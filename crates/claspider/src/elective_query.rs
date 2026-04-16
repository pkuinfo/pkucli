//! 选课系统课程查询（elective.pku.edu.cn "添加其它课程" 页面）
//!
//! 需要先通过 `elective login` 建立会话。
//!
//! ## API
//!
//! 首次查询：`POST /elective2008/edu/pku/stu/elective/controller/courseQuery/getCurriculmByForm.do`
//! 翻页：`GET /elective2008/edu/pku/stu/elective/controller/courseQuery/queryCurriculum.jsp?netui_row=syllabusListGrid;{offset}&...`
//!
//! 课程分类 radio 值：
//! - `education_plan_bk` — 培养方案
//! - `speciality` — 专业课
//! - `politics` — 政治课
//! - `english` — 英语课
//! - `gym` — 体育课
//! - `tsk_choice` — 通识课
//! - `pub_choice` — 公选课
//! - `liberal_computer` — 计算机基础课
//! - `ldjyk` — 劳动教育课
//! - `szxzxbx` — 思政选择性必修课

use crate::model::CourseInfo;
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use regex::Regex;
use scraper::{Html, Selector};

use pku_elective::login::APP_NAME;
use pkuinfo_common::session::Store;

const ELECTIVE_BASE: &str = "https://elective.pku.edu.cn/elective2008";
const COURSE_QUERY_FORM: &str = "https://elective.pku.edu.cn/elective2008/edu/pku/stu/elective/controller/courseQuery/getCurriculmByForm.do";
const COURSE_QUERY_PAGE: &str = "https://elective.pku.edu.cn/elective2008/edu/pku/stu/elective/controller/courseQuery/queryCurriculum.jsp";
const HELP_CONTROLLER: &str = "https://elective.pku.edu.cn/elective2008/edu/pku/stu/elective/controller/help/HelpController.jpf";

const PAGE_SIZE: usize = 100;

/// 从已保存的 elective session 构建 HTTP 客户端
fn build_client() -> Result<reqwest::Client> {
    let store = Store::new(APP_NAME)?;
    let session = store
        .load_session()?
        .ok_or_else(|| anyhow!("选课系统未登录。请先运行 `elective login`"))?;

    if session.is_expired() {
        return Err(anyhow!("选课系统会话已过期，请重新运行 `elective login`"));
    }

    let cookie_store = store.load_cookie_store()?;
    let client = pku_elective::client_build(cookie_store)?;
    Ok(client)
}

/// 解析课程查询结果 HTML 页面中的课程列表
fn parse_course_table(html: &str) -> Result<(Vec<CourseInfo>, usize)> {
    let dom = Html::parse_document(html);
    let table_sel = Selector::parse("table.datagrid").expect("static selector");
    let tr_sel = Selector::parse("tr").expect("static selector");
    let td_sel = Selector::parse("td").expect("static selector");

    let table = dom
        .select(&table_sel)
        .next()
        .ok_or_else(|| anyhow!("未找到课程查询结果表格"))?;

    let mut courses = Vec::new();

    for row in table.select(&tr_sel).skip(1) {
        let cells: Vec<String> = row
            .select(&td_sel)
            .map(|td| td.text().collect::<String>().trim().to_string())
            .collect();

        // 跳过分页行等无效行
        if cells.len() < 10 {
            continue;
        }

        // 列顺序：课程号(0) 课程名(1) 课程类别(2) 学分(3) 教师(4)
        //         班号(5) 开课单位(6) 专业(7) 年级(8) 上课时间及教室(9)
        //         限数/已选(10) 自选P/NP(11) 备注(12) 加入可选列表(13)
        let time_and_room = cells.get(9).cloned().unwrap_or_default();
        let (schedule, classroom, weeks) = parse_time_and_room(&time_and_room);

        courses.push(CourseInfo {
            course_id: cells[0].clone(),
            course_name: cells[1].clone(),
            category: cells[2].clone(),
            class_no: cells.get(5).cloned().unwrap_or_default(),
            credit: cells.get(3).cloned().unwrap_or_default(),
            teacher: cells.get(4).cloned().unwrap_or_default(),
            department: cells.get(6).cloned().unwrap_or_default(),
            schedule,
            classroom,
            weeks,
            remark: cells.get(12).cloned().unwrap_or_default(),
            source: "elective".to_string(),
            zhiyun_id: None,
        });
    }

    // 解析总页数
    let total_pages = parse_total_pages(&dom);

    Ok((courses, total_pages))
}

/// 从 "上课时间及教室" 字段提取 schedule、classroom、weeks
///
/// 格式示例：
/// - "1~15周 每周周二5~6节 理教310"
/// - "1~15周 双周周一1~2节 理教309考试方式：堂考"
/// - "1~15周 每周周三1~2节 三教2011~15周 每周周一3~4节 三教201"
/// - ""（空）
fn parse_time_and_room(raw: &str) -> (String, String, String) {
    if raw.is_empty() {
        return (String::new(), String::new(), String::new());
    }

    // 预处理：教室号和下一段周数可能粘连（如 "二教3171~15周"）
    // 周数格式 N~M周（N 通常 1~17），教室号通常 3+ 位数字。
    // 在 "3位+数字序列" + "1~2位数字~数字周" 粘连处插入空格。
    let raw = Regex::new(r"(\d{3,}?)(\d{1,2}~\d{1,2}周)")
        .expect("静态正则")
        .replace_all(raw, "$1 $2");
    let raw = raw.as_ref();

    let time_re = Regex::new(
        r"(\d{1,2})~(\d{1,2})周\s*((?:每周|单周|双周)?)周([一二三四五六日])(\d{1,2})~(\d{1,2})节",
    )
    .expect("静态正则");

    // 找出所有时间段的 byte 位置
    struct TimeSlot {
        full_match: String,
        weeks: String,
        byte_start: usize,
        byte_end: usize,
    }

    let mut slots: Vec<TimeSlot> = Vec::new();
    for caps in time_re.captures_iter(raw) {
        let m = caps.get(0).unwrap();
        let week_start = &caps[1];
        let week_end = &caps[2];
        slots.push(TimeSlot {
            full_match: m.as_str().to_string(),
            weeks: format!("{week_start}~{week_end}周"),
            byte_start: m.start(),
            byte_end: m.end(),
        });
    }

    if slots.is_empty() {
        return (raw.to_string(), String::new(), String::new());
    }

    let mut schedules = Vec::new();
    let mut classrooms = Vec::new();
    let weeks_str = slots[0].weeks.clone();

    for (i, slot) in slots.iter().enumerate() {
        schedules.push(slot.full_match.clone());

        // 教室名在这个时间段结束之后、下一个时间段开始之前
        let room_start = slot.byte_end;
        let room_end = if i + 1 < slots.len() {
            // 下一段的 byte_start 可能包含了教室号的尾部数字
            // 因为 time_re 匹配的 "(\d{1,2})~" 可能吃掉了教室号尾部
            // 所以真正的教室边界要回退到下一段 byte_start
            slots[i + 1].byte_start
        } else {
            raw.len()
        };

        if room_start < room_end {
            let room_raw = &raw[room_start..room_end];
            let room = room_raw
                .split("考试")
                .next()
                .unwrap_or("")
                .split("(备注")
                .next()
                .unwrap_or("")
                .split("（备注")
                .next()
                .unwrap_or("")
                .trim();
            if !room.is_empty() {
                classrooms.push(room.to_string());
            }
        }
    }

    // 修复教室号被下一段周数截断的问题
    // 如果第 i 个教室名为空，但下一段的 weeks 周数开头多了数字，
    // 说明教室号的尾部被吃掉了。不过这种情况比较难修复，
    // 因为无法区分 "教室501" + "1~15周" 中的 "1"。
    // 实际上，选课系统的原始数据里教室名和周数确实无分隔符粘连。
    // 退而求其次：如果多个教室名相同前缀，合并为一个。

    let schedule = schedules.join("; ");
    classrooms.dedup();
    let classroom = classrooms.join(", ");

    (schedule, classroom, weeks_str)
}

/// 解析总页数
fn parse_total_pages(dom: &Html) -> usize {
    let body_text: String = dom.root_element().text().collect();
    let re = Regex::new(r"Page\s*\d+\s*of\s*(\d+)").expect("静态正则");
    if let Some(caps) = re.captures(&body_text) {
        caps[1].parse().unwrap_or(1)
    } else {
        1
    }
}

/// 构建翻页 URL
fn page_url(category: &str, department: &str, offset: usize) -> String {
    format!(
        "{COURSE_QUERY_PAGE}?\
        netui_pagesize=syllabusListGrid%3B{PAGE_SIZE}&\
        netui_row=syllabusListGrid%3B{offset}&\
        wlw-radio_button_group_key%3A%7BactionForm.courseSettingType%7D={category}&\
        %7BactionForm.courseID%7D=&\
        %7BactionForm.courseName%7D=&\
        wlw-select_key%3A%7BactionForm.deptID%7DOldValue=true&\
        wlw-select_key%3A%7BactionForm.deptID%7D={department}&\
        wlw-select_key%3A%7BactionForm.courseDay%7DOldValue=true&\
        wlw-select_key%3A%7BactionForm.courseDay%7D=&\
        wlw-select_key%3A%7BactionForm.courseTime%7DOldValue=true&\
        wlw-select_key%3A%7BactionForm.courseTime%7D=&\
        wlw-checkbox_key%3A%7BactionForm.queryDateFlag%7DOldValue=false&\
        deptIdHide={department}"
    )
}

/// 跟随重定向并读取响应
async fn follow_and_read(client: &reqwest::Client, mut resp: reqwest::Response) -> Result<String> {
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
            return Err(anyhow!("会话已失效，请重新运行 `elective login`"));
        }

        let _ = resp.bytes().await?;
        resp = client.get(&location).send().await?;
    }

    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow!("请求失败 (HTTP {status})"));
    }
    let body = resp.text().await?;
    if body.contains("请重新登录") {
        return Err(anyhow!("会话已失效，请重新运行 `elective login`"));
    }
    Ok(body)
}

/// 从选课系统抓取指定分类的全部课程（自动分页）
pub async fn fetch_all(
    category: &str,
    department: Option<&str>,
    name_filter: Option<&str>,
) -> Result<Vec<CourseInfo>> {
    let client = build_client()?;
    let dept = department.unwrap_or("");

    // 第一页：POST 表单提交
    let form_body = format!(
        "wlw-radio_button_group_key%3A%7BactionForm.courseSettingType%7D={category}&\
        %7BactionForm.courseID%7D=&\
        %7BactionForm.courseName%7D=&\
        wlw-select_key%3A%7BactionForm.deptID%7DOldValue=true&\
        wlw-select_key%3A%7BactionForm.deptID%7D={dept}&\
        wlw-select_key%3A%7BactionForm.courseDay%7DOldValue=true&\
        wlw-select_key%3A%7BactionForm.courseDay%7D=&\
        wlw-select_key%3A%7BactionForm.courseTime%7DOldValue=true&\
        wlw-select_key%3A%7BactionForm.courseTime%7D=&\
        wlw-checkbox_key%3A%7BactionForm.queryDateFlag%7DOldValue=false&\
        deptIdHide={dept}",
    );

    let resp = client
        .post(COURSE_QUERY_FORM)
        .header("referer", HELP_CONTROLLER)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(form_body)
        .send()
        .await
        .context("选课系统查询请求失败")?;

    let html = follow_and_read(&client, resp).await?;
    let (mut courses, total_pages) = parse_course_table(&html)?;

    if total_pages > 1 {
        eprintln!("{} 选课系统共 {total_pages} 页，正在抓取...", "[*]".cyan());
    }

    // 翻页获取剩余数据
    for page in 1..total_pages {
        let offset = page * PAGE_SIZE;
        let url = page_url(category, dept, offset);
        let resp = client
            .get(&url)
            .header("referer", format!("{ELECTIVE_BASE}/edu/pku/stu/elective/controller/courseQuery/CourseQueryController.jpf"))
            .send()
            .await
            .with_context(|| format!("选课系统第 {} 页请求失败", page + 1))?;

        let html = follow_and_read(&client, resp).await?;
        let (page_courses, _) = parse_course_table(&html)?;
        courses.extend(page_courses);
    }

    // 按名称筛选
    if let Some(filter) = name_filter {
        courses.retain(|c| c.course_name.contains(filter));
    }

    Ok(courses)
}
