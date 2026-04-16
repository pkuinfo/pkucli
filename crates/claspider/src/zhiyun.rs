//! 智云课堂 API（onlineroomse.pku.edu.cn）
//!
//! 获取全校当日课程信息，包含精确的教室地点。
//!
//! ## 认证
//!
//! 智云课堂使用 JWT token 认证，token 来自教学网 (Blackboard) 的 SSO 登录。
//! cookie 设在 `.pku.edu.cn` 域下，所有子域共享。
//! 用户需要先在浏览器中通过教学网登录智云，然后把 JWT 粘贴给 CLI。
//!
//! ## API
//!
//! `GET /courseapi/v2/course-live/search-live-course-list`
//! - `search_time=YYYY-MM-DD` — 查询日期
//! - `tenant=226` — 租户（固定值）
//! - `need_time_quantum=1&unique_course=1&with_sub_duration=1&with_sub_data=1`
//!
//! 返回该天所有课程，按时段分组。通过遍历一周（周一到周日）并去重，可获取全部课程。
//!
//! ## 课程详情
//!
//! `GET /courseapi/v2/schedule/search-live-course-list`
//! - `course_id=N&all=1&with_sub_data=1&with_room_data=1&show_all=1&show_delete=2`
//!
//! 返回的 `information` JSON 字段包含 `kcdm`（课程代码，即教务部课程号）。

use crate::model::CourseInfo;
use anyhow::{anyhow, Context, Result};
use chrono::Datelike;
use colored::Colorize;
use serde::Deserialize;
use std::collections::HashMap;

const ZHIYUN_BASE: &str = "https://onlineroomse.pku.edu.cn";
const ZHIYUN_API_BASE: &str = "https://onlineroomse.pku.edu.cn/courseapi/v2";

/// 智云 API 通用响应
#[derive(Debug, Deserialize)]
struct ZhiyunResponse<T> {
    code: i32,
    #[serde(default)]
    _msg: String,
    #[serde(default)]
    list: Vec<T>,
}

/// 时段
#[derive(Debug, Default, Deserialize)]
struct TimeSlot {
    #[serde(default)]
    list: Vec<ZhiyunCourse>,
}

/// 课程列表中的课程
#[derive(Debug, Deserialize)]
struct ZhiyunCourse {
    id: String,
    title: String,
    #[serde(default)]
    lecturer_name: String,
    #[serde(default)]
    room_name: String,
    #[serde(default)]
    sub_title: String,
}

/// 课程详情（含 information 字段）
#[derive(Debug, Default, Deserialize)]
struct ZhiyunCourseDetail {
    #[serde(default)]
    information: Option<String>,
    #[serde(default)]
    kkxy_name: String,
    #[serde(default)]
    structure_name: String,
}

/// information JSON 内的字段
#[derive(Debug, Deserialize)]
struct CourseInformation {
    /// 课程代码（= 教务部课程号，如 "04830220"）
    #[serde(default)]
    kcdm: String,
    /// 课程唯一编码（如 "25262-00048-04830220-0006168313-00-1"）
    #[serde(default)]
    kcwybm: String,
}

fn build_client(jwt: &str) -> Result<reqwest::Client> {
    use reqwest::header::{HeaderMap, HeaderValue};
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {jwt}")).context("JWT token 格式错误")?,
    );
    headers.insert(
        "accept",
        HeaderValue::from_static("application/json, text/plain, */*"),
    );
    headers.insert("referer", HeaderValue::from_static(ZHIYUN_BASE));
    headers.insert(
        "user-agent",
        HeaderValue::from_static(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36",
        ),
    );
    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .context("构建智云 HTTP 客户端失败")
}

/// 简化教室名：去掉 "北京大学本部" 等前缀
fn simplify_room_name(raw: &str) -> String {
    raw.replace("北京大学本部", "")
        .replace("默认校区默认教学楼-默认教室", "")
        .trim_start_matches('-')
        .to_string()
}

/// 从 sub_title 提取节次信息（如 "2026-04-14第3-4节" → "第3-4节"）
fn extract_period(sub_title: &str) -> String {
    if let Some(idx) = sub_title.find('第') {
        sub_title[idx..].to_string()
    } else {
        sub_title.to_string()
    }
}

/// 从 kcwybm 提取班号
/// 格式: "25262-00048-04830220-0006168313-00-1" → 最后一位 "1"
fn extract_class_no_from_kcwybm(kcwybm: &str) -> String {
    kcwybm.rsplit('-').next().unwrap_or("").to_string()
}

/// 查询某一天的全部课程
async fn fetch_day(client: &reqwest::Client, date: &str) -> Result<Vec<ZhiyunCourse>> {
    let url = format!(
        "{ZHIYUN_API_BASE}/course-live/search-live-course-list?\
        need_time_quantum=1&unique_course=1&with_sub_duration=1\
        &search_time={date}&tenant=226\
        &course_student_type=&sub_live_status=&with_sub_data=1"
    );

    let resp = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("智云查询 {date} 失败"))?;

    let data: ZhiyunResponse<TimeSlot> = resp
        .json()
        .await
        .with_context(|| format!("解析智云 {date} 响应失败"))?;

    if data.code != 0 {
        return Err(anyhow!("智云 API 返回错误: code={}", data.code));
    }

    let mut courses = Vec::new();
    for slot in data.list {
        courses.extend(slot.list);
    }
    Ok(courses)
}

/// 查询课程详情（含 information 字段中的课程代码）
async fn fetch_course_detail(
    client: &reqwest::Client,
    course_id: &str,
) -> Result<ZhiyunCourseDetail> {
    let url = format!(
        "{ZHIYUN_API_BASE}/schedule/search-live-course-list?\
        all=1&course_id={course_id}&with_sub_data=1&with_room_data=1&show_all=1&show_delete=2"
    );

    let resp = client.get(&url).send().await?;
    let data: ZhiyunResponse<ZhiyunCourseDetail> = resp.json().await?;

    data.list
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("课程 {course_id} 详情为空"))
}

/// 从智云抓取一周的课程并去重，返回统一的 CourseInfo 列表
///
/// `week_start` 格式为 "YYYY-MM-DD"（周一日期），会查询周一到周日共 7 天。
/// `fetch_details` 为 true 时，会逐个查询课程详情以获取课程代码（耗时较长）。
pub async fn fetch_all(
    jwt: &str,
    week_start: &str,
    fetch_details: bool,
) -> Result<Vec<CourseInfo>> {
    let client = build_client(jwt)?;

    // 解析起始日期
    let start = chrono::NaiveDate::parse_from_str(week_start, "%Y-%m-%d")
        .with_context(|| format!("日期格式错误: {week_start}，应为 YYYY-MM-DD"))?;

    // 遍历 7 天
    let mut all_courses: HashMap<String, ZhiyunCourse> = HashMap::new();
    for day_offset in 0..7 {
        let date = start + chrono::Duration::days(day_offset);
        let date_str = date.format("%Y-%m-%d").to_string();
        let day_name = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
            [date.weekday().num_days_from_monday() as usize];

        let courses = fetch_day(&client, &date_str).await?;
        let count = courses.len();

        for c in courses {
            all_courses.entry(c.id.clone()).or_insert(c);
        }

        eprintln!(
            "  {day_name} {date_str}: {count} 门课程（累计去重 {} 门）",
            all_courses.len()
        );
    }

    eprintln!(
        "{} 智云共获取 {} 门唯一课程",
        "[*]".cyan(),
        all_courses.len()
    );

    // 如果需要详情（获取课程代码），逐个查询
    let mut results = Vec::with_capacity(all_courses.len());

    if fetch_details {
        eprintln!("{} 正在查询课程详情（获取课程代码）...", "[*]".cyan());
        let total = all_courses.len();
        for (i, (id, c)) in all_courses.into_iter().enumerate() {
            if (i + 1) % 50 == 0 || i + 1 == total {
                eprintln!("  进度: {}/{total}", i + 1);
            }

            let (course_code, class_no, dept) = match fetch_course_detail(&client, &id).await {
                Ok(detail) => {
                    let info: Option<CourseInformation> = detail
                        .information
                        .as_deref()
                        .and_then(|s| serde_json::from_str(s).ok());

                    let code = info.as_ref().map(|i| i.kcdm.clone()).unwrap_or_default();
                    let class = info
                        .as_ref()
                        .map(|i| extract_class_no_from_kcwybm(&i.kcwybm))
                        .unwrap_or_default();
                    let dept = if detail.kkxy_name.is_empty() {
                        detail.structure_name
                    } else {
                        detail.kkxy_name
                    };
                    (code, class, dept)
                }
                Err(_) => (String::new(), String::new(), String::new()),
            };

            results.push(CourseInfo {
                course_id: course_code,
                course_name: c.title,
                category: String::new(),
                class_no,
                credit: String::new(),
                teacher: c.lecturer_name,
                department: dept,
                schedule: extract_period(&c.sub_title),
                classroom: simplify_room_name(&c.room_name),
                weeks: String::new(),
                remark: String::new(),
                source: "zhiyun".to_string(),
                zhiyun_id: Some(c.id),
            });
        }
    } else {
        // 快速模式：不查详情，没有课程代码
        for (id, c) in all_courses {
            results.push(CourseInfo {
                course_id: String::new(),
                course_name: c.title,
                category: String::new(),
                class_no: String::new(),
                credit: String::new(),
                teacher: c.lecturer_name,
                department: String::new(),
                schedule: extract_period(&c.sub_title),
                classroom: simplify_room_name(&c.room_name),
                weeks: String::new(),
                remark: String::new(),
                source: "zhiyun".to_string(),
                zhiyun_id: Some(id),
            });
        }
    }

    Ok(results)
}
