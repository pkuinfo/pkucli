//! 选课网 API 客户端
//!
//! 主要功能：
//! - 查看选课结果
//! - 浏览补退选课程列表（分页）
//! - 获取验证码 + 验证
//! - 提交选课

use anyhow::{anyhow, Context, Result};
use scraper::{Html, Selector};

use crate::client::{self, ELECTIVE_BASE};
use crate::config::ElectiveConfig;
use pkuinfo_common::session::Store;

use crate::login::APP_NAME;

// ─── URL 常量 ─────────────────────────────────────────────────

const SHOW_RESULTS: &str =
    "https://elective.pku.edu.cn/elective2008/edu/pku/stu/elective/controller/electiveWork/showResults.do";
const HELP_CONTROLLER: &str =
    "https://elective.pku.edu.cn/elective2008/edu/pku/stu/elective/controller/help/HelpController.jpf";
const SUPPLEMENT: &str =
    "https://elective.pku.edu.cn/elective2008/edu/pku/stu/elective/controller/supplement/supplement.jsp";
const SUPPLY_CANCEL: &str =
    "https://elective.pku.edu.cn/elective2008/edu/pku/stu/elective/controller/supplement/SupplyCancel.do";
const DRAW_SERVLET: &str = "https://elective.pku.edu.cn/elective2008/DrawServlet";
const VALIDATE: &str =
    "https://elective.pku.edu.cn/elective2008/edu/pku/stu/elective/controller/supplement/validate.do";

// ─── 数据模型 ──────────────────────────────────────────────────

/// 课程基础信息（选课结果 / 已选课程）
#[derive(Debug, Clone)]
pub struct CourseData {
    /// 课程名
    pub name: String,
    /// 课程类别
    pub category: String,
    /// 学分
    pub credit: String,
    /// 周学时
    pub hours: String,
    /// 教师
    pub teacher: String,
    /// 班号
    pub class_id: String,
    /// 开课单位
    pub department: String,
    /// 教室信息 / 上课考试信息
    pub classroom: String,
    /// 选课结果 / 限数已选
    pub status: String,
}

/// 补退选课程信息（带选课链接）
#[derive(Debug, Clone)]
pub struct SupplementCourse {
    /// 课程基础信息
    pub base: CourseData,
    /// 选课操作 URL（相对路径）
    pub elect_url: String,
    /// 所在页码（0-indexed）
    pub page_id: usize,
}

impl SupplementCourse {
    /// 是否满员: 解析 status 字段的 "限数/已选" 格式
    pub fn is_full(&self) -> bool {
        let parts: Vec<&str> = self.base.status.split('/').collect();
        if parts.len() >= 2 {
            let limit: usize = parts[0].trim().parse().unwrap_or(0);
            let selected: usize = parts[1].trim().parse().unwrap_or(0);
            selected >= limit
        } else {
            false
        }
    }
}

/// 验证码验证结果
#[derive(Debug)]
pub enum ValidationResult {
    /// 验证成功
    Success,
    /// 未填写
    Empty,
    /// 验证码错误
    Wrong,
}

// ─── API 客户端 ────────────────────────────────────────────────

pub struct ElectiveApi {
    client: reqwest::Client,
    username: String,
}

impl ElectiveApi {
    /// 从已保存的 session 构建 API 客户端
    pub fn from_session() -> Result<Self> {
        let store = Store::new(APP_NAME)?;
        let session = store
            .load_session()?
            .ok_or_else(|| anyhow!("未登录。请先运行 `elective login`"))?;

        if session.is_expired() {
            return Err(anyhow!("会话已过期，请重新登录"));
        }

        let cookie_store = store.load_cookie_store()?;
        let client = client::build(cookie_store)?;

        let username = session
            .uid
            .or_else(|| {
                ElectiveConfig::load(store.config_dir())
                    .ok()
                    .and_then(|c| c.username)
            })
            .unwrap_or_default();

        Ok(Self { client, username })
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    // ─── 选课结果查询 ───────────────────────────────────────────

    /// 获取选课结果页面
    pub async fn get_results(&self) -> Result<Vec<CourseData>> {
        let resp = self
            .client
            .get(SHOW_RESULTS)
            .header("referer", HELP_CONTROLLER)
            .send()
            .await
            .context("获取选课结果失败")?;

        let body = self.follow_and_read(resp).await?;
        let dom = Html::parse_document(&body);

        parse_datagrid_table(&dom, 0, &RESULT_COLUMNS)
    }

    // ─── 补退选 ─────────────────────────────────────────────────

    /// 获取补退选页面的总页数和已选课程
    pub async fn get_supply_cancel(&self) -> Result<(usize, Vec<CourseData>)> {
        let url = format!("{SUPPLY_CANCEL}?xh={}", self.username);
        let resp = self
            .client
            .get(&url)
            .header("referer", HELP_CONTROLLER)
            .header("cache-control", "max-age=0")
            .send()
            .await
            .context("获取补退选页面失败")?;

        let body = self.follow_and_read(resp).await?;
        let dom = Html::parse_document(&body);

        // 总页数: 从分页控件解析
        let total_pages = parse_total_pages(&dom);

        // 已选课程: 第二个 table.datagrid
        let elected = parse_datagrid_table(&dom, 1, &ELECTED_COLUMNS).unwrap_or_default();

        Ok((total_pages, elected))
    }

    /// 获取补退选课程列表（指定页码）
    pub async fn get_supplements(&self, page: usize) -> Result<Vec<SupplementCourse>> {
        let url = format!(
            "{SUPPLEMENT}?xh={}&netui_row=electableListGrid;{}",
            self.username,
            page * 20,
        );
        let resp = self
            .client
            .get(&url)
            .header("referer", SUPPLY_CANCEL)
            .header("cache-control", "max-age=0")
            .send()
            .await
            .context("获取补退选列表失败")?;

        let body = self.follow_and_read(resp).await?;
        let dom = Html::parse_document(&body);

        parse_supplement_table(&dom, page)
    }

    /// 获取所有补退选课程（遍历所有页面）
    pub async fn get_all_supplements(&self) -> Result<Vec<SupplementCourse>> {
        let (total_pages, _) = self.get_supply_cancel().await?;

        let mut all = Vec::new();
        for page in 0..total_pages {
            let courses = self.get_supplements(page).await?;
            all.extend(courses);
        }

        Ok(all)
    }

    // ─── 验证码 ─────────────────────────────────────────────────

    /// 获取验证码图片（JPEG 字节）
    pub async fn get_captcha_image(&self) -> Result<Vec<u8>> {
        let rand_val: f64 = rand::random();
        let url = format!("{DRAW_SERVLET}?Rand={rand_val:.20}");

        let resp = self
            .client
            .get(&url)
            .header("referer", SUPPLY_CANCEL)
            .send()
            .await
            .context("获取验证码图片失败")?;

        let bytes = resp.bytes().await?.to_vec();
        Ok(bytes)
    }

    /// 提交验证码验证
    pub async fn validate_captcha(&self, code: &str) -> Result<ValidationResult> {
        let body = format!("xh={}&validCode={code}", self.username);
        let resp = self
            .client
            .post(VALIDATE)
            .header("referer", SUPPLY_CANCEL)
            .header(
                "content-type",
                "application/x-www-form-urlencoded; charset=UTF-8",
            )
            .body(body)
            .send()
            .await
            .context("验证码验证请求失败")?;

        let text = self.follow_and_read(resp).await?;

        // 响应格式: { "valid": "2" } — 2=成功, 1=未填写, 0=错误
        let json: serde_json::Value = serde_json::from_str(&text).context("验证码响应解析失败")?;

        let valid = json["valid"].as_str().unwrap_or("0");

        match valid {
            "2" => Ok(ValidationResult::Success),
            "1" => Ok(ValidationResult::Empty),
            _ => Ok(ValidationResult::Wrong),
        }
    }

    // ─── 选课提交 ───────────────────────────────────────────────

    /// 提交选课请求
    pub async fn elect(&self, elect_url: &str) -> Result<Option<String>> {
        let full_url = format!("{ELECTIVE_BASE}{elect_url}");
        let resp = self
            .client
            .get(&full_url)
            .header("referer", SUPPLY_CANCEL)
            .send()
            .await
            .context("选课请求失败")?;

        let body = self.follow_and_read(resp).await?;
        let dom = Html::parse_document(&body);

        // 结果消息在 td#msgTips 中
        let sel = Selector::parse("td#msgTips").expect("static selector");
        let msg = dom
            .select(&sel)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string());

        Ok(msg)
    }

    // ─── 辅助 ───────────────────────────────────────────────────

    /// 跟随重定向并读取最终页面内容
    async fn follow_and_read(&self, mut resp: reqwest::Response) -> Result<String> {
        for _ in 0..5 {
            if !resp.status().is_redirection() {
                break;
            }
            let location = resp
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| anyhow!("重定向缺少 Location 头"))?
                .to_string();

            if location.contains("iaaa") || location.contains("login") {
                return Err(anyhow!(
                    "会话已失效（被重定向到登录页），请重新运行 `elective login`"
                ));
            }

            let _ = resp.bytes().await?;

            resp = self
                .client
                .get(&location)
                .send()
                .await
                .with_context(|| format!("重定向请求失败: {location}"))?;
        }

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(anyhow!("会话已失效，请重新运行 `elective login`"));
        }
        if !status.is_success() {
            return Err(anyhow!("请求失败 (HTTP {status})"));
        }

        let body = resp.text().await.context("读取响应内容失败")?;

        if body.contains("IAAA") && body.contains("login") {
            return Err(anyhow!(
                "会话已失效（被重定向到登录页），请重新运行 `elective login`"
            ));
        }

        Ok(body)
    }
}

// ─── HTML 解析 ─────────────────────────────────────────────────

/// 选课结果表格列
const RESULT_COLUMNS: [&str; 10] = [
    "课程号",
    "课程名",
    "课程类别",
    "学分",
    "周学时",
    "教师",
    "班号",
    "开课单位",
    "教室信息",
    "选课结果",
];

/// 已选课程表格列
const ELECTED_COLUMNS: [&str; 10] = [
    "课程号",
    "课程名",
    "课程类别",
    "学分",
    "周学时",
    "教师",
    "班号",
    "开课单位",
    "年级",
    "限数/已选",
];

/// 解析 table.datagrid 中的课程数据
fn parse_datagrid_table(
    dom: &Html,
    table_index: usize,
    _expected_cols: &[&str],
) -> Result<Vec<CourseData>> {
    let table_sel = Selector::parse("table.datagrid").expect("static selector");
    let tr_sel = Selector::parse("tr").expect("static selector");
    let td_sel = Selector::parse("td").expect("static selector");

    let table = dom
        .select(&table_sel)
        .nth(table_index)
        .ok_or_else(|| anyhow!("未找到第 {} 个 datagrid 表格", table_index + 1))?;

    let mut courses = Vec::new();

    // 跳过表头行
    for row in table.select(&tr_sel).skip(1) {
        let cells: Vec<String> = row
            .select(&td_sel)
            .map(|td| td.text().collect::<String>().trim().to_string())
            .collect();

        // 过滤分页行等
        if cells.len() <= 3 {
            continue;
        }

        // 至少需要 10 列（前面有课程号，我们从 index 1 开始取）
        if cells.len() < 10 {
            continue;
        }

        courses.push(CourseData {
            name: cells[1].clone(),
            category: cells[2].clone(),
            credit: cells[3].clone(),
            hours: cells[4].clone(),
            teacher: cells[5].clone(),
            class_id: cells[6].clone(),
            department: cells[7].clone(),
            classroom: cells[8].clone(),
            status: cells[9].clone(),
        });
    }

    Ok(courses)
}

/// 解析补退选课程表格（第一个 table.datagrid，含选课链接）
fn parse_supplement_table(dom: &Html, page: usize) -> Result<Vec<SupplementCourse>> {
    let table_sel = Selector::parse("table.datagrid").expect("static selector");
    let tr_sel = Selector::parse("tr").expect("static selector");
    let td_sel = Selector::parse("td").expect("static selector");
    let a_sel = Selector::parse("a").expect("static selector");

    let table = dom
        .select(&table_sel)
        .next()
        .ok_or_else(|| anyhow!("未找到补退选课程表格"))?;

    let mut courses = Vec::new();

    for row in table.select(&tr_sel).skip(1) {
        let cells: Vec<scraper::ElementRef> = row.select(&td_sel).collect();

        if cells.len() <= 2 {
            continue;
        }

        let texts: Vec<String> = cells
            .iter()
            .map(|td| td.text().collect::<String>().trim().to_string())
            .collect();

        if texts.len() < 10 {
            continue;
        }

        // 选课链接在最后一个 td 的第一个 <a> 中
        let elect_url = cells
            .last()
            .and_then(|td| td.select(&a_sel).next())
            .and_then(|a| a.value().attr("href"))
            .unwrap_or_default()
            .to_string();

        // status 可能是 "限数/已选" 或 "限数/已选/候补"
        let status_idx = texts.len().saturating_sub(3).max(9);
        let status = texts.get(status_idx).cloned().unwrap_or_default();

        courses.push(SupplementCourse {
            base: CourseData {
                name: texts[1].clone(),
                category: texts[2].clone(),
                credit: texts[3].clone(),
                hours: texts[4].clone(),
                teacher: texts[5].clone(),
                class_id: texts[6].clone(),
                department: texts[7].clone(),
                classroom: texts[8].clone(),
                status,
            },
            elect_url,
            page_id: page,
        });
    }

    Ok(courses)
}

/// 从分页控件解析总页数
fn parse_total_pages(dom: &Html) -> usize {
    let sel = Selector::parse("tr[align='right'] > td").expect("static selector");
    let re = regex::Regex::new(r"Page\s*\d+?\s*of\s*(\d+?)").expect("static regex");
    for td in dom.select(&sel) {
        let text = td.text().collect::<String>();
        if let Some(caps) = re.captures(&text) {
            if let Ok(n) = caps[1].parse::<usize>() {
                return n;
            }
        }
    }
    1 // 默认 1 页
}
