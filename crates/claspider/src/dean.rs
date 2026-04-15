//! 教务部课表查询 API（dean.pku.edu.cn）
//!
//! 无需登录，但有验证码限制（仅前端检查，直接 POST 可绕过）。
//! 每次请求返回 10 条，需要分页遍历。
//!
//! ## API
//!
//! `POST /service/web/courseSearch_do.php`
//!
//! | 参数 | 说明 |
//! |------|------|
//! | coursename | 课程名关键词 |
//! | teachername | 教师姓名 |
//! | yearandseme | 学年学期，如 "25-26-2" |
//! | coursetype | 课程类型代码，"0"=全部 |
//! | yuanxi | 院系代码，"0"=全部 |
//! | startrow | 分页偏移量 |
//!
//! 响应 JSON：
//! ```json
//! {
//!   "status": "ok",
//!   "count": "223",
//!   "courselist": [{ "kch", "kcmc", "kctxm", "kkxsmc", "jxbh", "xf", "qzz", "sksj", "teacher", "bz", "zxjhbh", "xh" }]
//! }
//! ```

use crate::model::CourseInfo;
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use regex::Regex;
use serde::Deserialize;

const DEAN_BASE: &str = "https://dean.pku.edu.cn/service/web";

/// 教务部课程查询 API 响应
#[derive(Debug, Deserialize)]
struct DeanResponse {
    status: String,
    #[serde(default)]
    count: String,
    #[serde(default)]
    courselist: Vec<DeanCourse>,
}

/// 教务部课程数据
#[derive(Debug, Deserialize)]
struct DeanCourse {
    /// 课程号
    kch: String,
    /// 课程名称
    kcmc: String,
    /// 课程类型（专业必修/专业限选/全校任选 等）
    kctxm: String,
    /// 开课单位
    kkxsmc: String,
    /// 班号
    jxbh: String,
    /// 学分
    xf: String,
    /// 起止周
    qzz: String,
    /// 上课时间（HTML 格式，如 "<p>星期二(第3节-第4节)</p>"）
    sksj: String,
    /// 教师（HTML 格式）
    teacher: String,
    /// 备注
    bz: String,
}

/// 从 HTML 标签中提取纯文本
fn strip_html(html: &str) -> String {
    let re = Regex::new(r"<[^>]+>").expect("静态正则");
    let text = re.replace_all(html, " ");
    // 合并多余空白
    let re2 = Regex::new(r"\s+").expect("静态正则");
    re2.replace_all(text.trim(), " ").to_string()
}

/// 从教务部查询中提取教室信息（备注字段中可能包含）
fn extract_classroom_from_remark(bz: &str) -> String {
    // 部分课程在备注中包含教室信息，格式多样，暂不提取
    // 返回空，教室信息主要靠选课系统补充
    let _ = bz;
    String::new()
}

impl DeanCourse {
    fn into_course_info(self) -> CourseInfo {
        let schedule = strip_html(&self.sksj);
        let teacher = strip_html(&self.teacher);
        let classroom = extract_classroom_from_remark(&self.bz);

        CourseInfo {
            course_id: self.kch,
            course_name: self.kcmc,
            category: self.kctxm,
            class_no: self.jxbh,
            credit: self.xf,
            teacher,
            department: self.kkxsmc,
            schedule,
            classroom,
            weeks: self.qzz,
            remark: self.bz,
            source: "dean".to_string(),
            zhiyun_id: None,
        }
    }
}

fn build_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36")
        .build()
        .context("构建 HTTP 客户端失败")
}

/// 查询一页课程数据
async fn fetch_page(
    client: &reqwest::Client,
    semester: &str,
    department: &str,
    coursename: &str,
    teachername: &str,
    startrow: usize,
) -> Result<DeanResponse> {
    let resp = client
        .post(format!("{DEAN_BASE}/courseSearch_do.php"))
        .header("referer", format!("{DEAN_BASE}/courseSearch.php"))
        .header("origin", "https://dean.pku.edu.cn")
        .header("x-requested-with", "XMLHttpRequest")
        .form(&[
            ("coursename", coursename),
            ("teachername", teachername),
            ("yearandseme", semester),
            ("coursetype", "0"),
            ("yuanxi", department),
            ("startrow", &startrow.to_string()),
        ])
        .send()
        .await
        .context("教务部查询请求失败")?;

    let text = resp.text().await.context("读取教务部响应失败")?;
    let data: DeanResponse =
        serde_json::from_str(&text).with_context(|| format!("解析教务部响应失败: {text}"))?;

    if data.status != "ok" {
        return Err(anyhow!("教务部返回错误状态: {}", data.status));
    }

    Ok(data)
}

/// 从教务部抓取全部课程（自动分页）
pub async fn fetch_all(
    semester: &str,
    department: Option<&str>,
    coursename: Option<&str>,
    teachername: Option<&str>,
) -> Result<Vec<CourseInfo>> {
    let client = build_client()?;
    let dept = department.unwrap_or("0");
    let name = coursename.unwrap_or("");
    let teacher = teachername.unwrap_or("");

    // 第一页：获取总数
    let first = fetch_page(&client, semester, dept, name, teacher, 0).await?;
    let total: usize = first.count.parse().unwrap_or(0);

    if total == 0 {
        eprintln!("{} 教务部未查到任何课程", "[!]".yellow());
        return Ok(Vec::new());
    }

    let mut courses: Vec<CourseInfo> = first
        .courselist
        .into_iter()
        .map(|c| c.into_course_info())
        .collect();

    let page_size = 10;
    let total_pages = total.div_ceil(page_size);

    if total_pages > 1 {
        eprintln!(
            "{} 教务部共 {total} 门课程，{total_pages} 页，正在抓取...",
            "[*]".cyan()
        );
    }

    for page in 1..total_pages {
        let offset = page * page_size;
        let data = fetch_page(&client, semester, dept, name, teacher, offset).await?;
        courses.extend(data.courselist.into_iter().map(|c| c.into_course_info()));
    }

    Ok(courses)
}
