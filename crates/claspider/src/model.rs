//! 统一课程数据模型

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 统一的课程信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourseInfo {
    /// 课程号（教务部 kch / 智云 kcdm）
    pub course_id: String,
    /// 课程名称
    pub course_name: String,
    /// 课程类别（专业必修/专业限选/全校任选 等）
    pub category: String,
    /// 班号
    pub class_no: String,
    /// 学分
    pub credit: String,
    /// 教师
    pub teacher: String,
    /// 开课单位
    pub department: String,
    /// 上课时间（如 "星期二(第3节-第4节)"）
    pub schedule: String,
    /// 教室地点（如 "理教309"），可能为空
    pub classroom: String,
    /// 起止周（如 "1-15"）
    pub weeks: String,
    /// 备注
    pub remark: String,
    /// 数据来源（dean / elective / zhiyun / dean+elective+zhiyun 等）
    pub source: String,
    /// 智云课堂课程 ID（可选，用于追溯）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zhiyun_id: Option<String>,
}

/// 合并三个渠道的课程数据
///
/// 1. 教务部（基底，最全）
/// 2. 选课系统（补充教室）
/// 3. 智云课堂（补充/验证教室，需 JWT）
///
/// 匹配键：(课程号, 班号)
pub async fn merge_sources(
    semester: &str,
    elective_category: &str,
    department: Option<&str>,
    zhiyun_jwt: Option<&str>,
    zhiyun_week: Option<&str>,
) -> anyhow::Result<Vec<CourseInfo>> {
    use colored::Colorize;

    let step_total = if zhiyun_jwt.is_some() { 4 } else { 3 };

    eprintln!(
        "{} 从教务部抓取课程列表...",
        format!("[1/{step_total}]").cyan()
    );
    let dean_courses = crate::dean::fetch_all(semester, department, None, None).await?;
    eprintln!("  获取 {} 门课程", dean_courses.len());

    eprintln!(
        "{} 从选课系统抓取课程列表...",
        format!("[2/{step_total}]").cyan()
    );
    let elective_courses =
        match crate::elective_query::fetch_all(elective_category, department, None).await {
            Ok(c) => {
                eprintln!("  获取 {} 门课程", c.len());
                c
            }
            Err(e) => {
                eprintln!("  {} 选课系统获取失败: {e}（跳过）", "[!]".yellow());
                Vec::new()
            }
        };

    let zhiyun_courses = if let Some(jwt) = zhiyun_jwt {
        let week = zhiyun_week.unwrap_or("2026-04-13");
        eprintln!(
            "{} 从智云课堂抓取课程列表 (起始 {week})...",
            format!("[3/{step_total}]").cyan()
        );
        match crate::zhiyun::fetch_all(jwt, week, false).await {
            Ok(c) => {
                eprintln!("  获取 {} 门课程", c.len());
                c
            }
            Err(e) => {
                eprintln!("  {} 智云课堂获取失败: {e}（跳过）", "[!]".yellow());
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    eprintln!(
        "{} 合并数据...",
        format!("[{step_total}/{step_total}]").cyan()
    );

    // 建索引：(课程号, 班号) → 课程
    let mut elective_map: HashMap<(String, String), &CourseInfo> = HashMap::new();
    for c in &elective_courses {
        if !c.course_id.is_empty() {
            elective_map.insert((c.course_id.clone(), c.class_no.clone()), c);
        }
    }

    let mut zhiyun_map: HashMap<(String, String), &CourseInfo> = HashMap::new();
    // 也建一个 name+teacher 的模糊索引，用于 course_id 为空时的匹配
    let mut zhiyun_name_map: HashMap<(String, String), &CourseInfo> = HashMap::new();
    for c in &zhiyun_courses {
        if !c.course_id.is_empty() {
            zhiyun_map.insert((c.course_id.clone(), c.class_no.clone()), c);
        }
        zhiyun_name_map.insert((c.course_name.clone(), c.teacher.clone()), c);
    }

    let mut merged: Vec<CourseInfo> = Vec::with_capacity(dean_courses.len());
    let mut enriched_elective = 0usize;
    let mut enriched_zhiyun = 0usize;

    for mut dc in dean_courses {
        let key = (dc.course_id.clone(), dc.class_no.clone());
        let mut sources = vec!["dean"];

        // 用选课系统补充
        if let Some(ec) = elective_map.remove(&key) {
            if dc.classroom.is_empty() && !ec.classroom.is_empty() {
                dc.classroom = ec.classroom.clone();
                enriched_elective += 1;
            }
            sources.push("elective");
        }

        // 用智云补充
        let zy = zhiyun_map
            .remove(&key)
            .or_else(|| zhiyun_name_map.remove(&(dc.course_name.clone(), dc.teacher.clone())));
        if let Some(zc) = zy {
            if dc.classroom.is_empty() && !zc.classroom.is_empty() {
                dc.classroom = zc.classroom.clone();
                enriched_zhiyun += 1;
            }
            dc.zhiyun_id = zc.zhiyun_id.clone();
            sources.push("zhiyun");
        }

        dc.source = sources.join("+");
        merged.push(dc);
    }

    // 添加仅在选课系统的课程
    let extra_elective = elective_map.len();
    for (_, ec) in elective_map {
        merged.push(ec.clone());
    }

    // 添加仅在智云的课程
    let extra_zhiyun = zhiyun_map.len();
    for (_, zc) in zhiyun_map {
        merged.push(zc.clone());
    }

    eprintln!(
        "  合并完成：共 {} 门 | 选课补教室 {} | 智云补教室 {} | 仅选课 {} | 仅智云 {}",
        merged.len(),
        enriched_elective,
        enriched_zhiyun,
        extra_elective,
        extra_zhiyun,
    );

    Ok(merged)
}
