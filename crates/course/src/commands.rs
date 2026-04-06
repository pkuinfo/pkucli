//! 所有子命令的实现

use crate::api::CourseApi;
use crate::display;
use anyhow::{anyhow, Result};
use colored::Colorize;
use std::path::Path;

// ─── courses ───────────────────────────────────────────────────

pub async fn cmd_courses(all: bool) -> Result<()> {
    let api = CourseApi::from_session()?;
    let courses = api.list_courses(!all).await?;

    if courses.is_empty() {
        println!("{}", "暂无课程".dimmed());
        return Ok(());
    }

    display::print_courses(&courses);
    Ok(())
}

// ─── info ──────────────────────────────────────────────────────

pub async fn cmd_info(course_id: &str) -> Result<()> {
    let api = CourseApi::from_session()?;

    // 如果传入的是数字索引，先查询课程列表
    let actual_id = resolve_course_id(&api, course_id).await?;

    let entries = api.list_course_entries(&actual_id).await?;

    if entries.is_empty() {
        println!("{}", "课程侧边栏无内容".dimmed());
        return Ok(());
    }

    display::print_course_entries(&actual_id, &entries);
    Ok(())
}

// ─── content ───────────────────────────────────────────────────

pub async fn cmd_content(course_id: &str, content_id: &str) -> Result<()> {
    let api = CourseApi::from_session()?;
    let actual_course = resolve_course_id(&api, course_id).await?;

    let items = api.list_content(&actual_course, content_id).await?;
    display::print_content_list(&items);
    Ok(())
}

// ─── assignment ────────────────────────────────────────────────

pub async fn cmd_assignment(course_id: &str, content_id: &str) -> Result<()> {
    let api = CourseApi::from_session()?;
    let actual_course = resolve_course_id(&api, course_id).await?;

    let detail = api.get_assignment(&actual_course, content_id).await?;
    display::print_assignment_detail(&detail);
    Ok(())
}

// ─── download ──────────────────────────────────────────────────

pub async fn cmd_download(url: &str, output_dir: Option<&str>) -> Result<()> {
    let api = CourseApi::from_session()?;

    println!("{} 正在下载...", "[*]".cyan());
    let (filename, bytes) = api.download_file(url).await?;

    let out_dir = output_dir.unwrap_or(".");
    let out_path = Path::new(out_dir).join(&filename);

    tokio::fs::create_dir_all(out_dir).await?;
    tokio::fs::write(&out_path, &bytes).await?;

    println!(
        "{} 已下载: {} ({})",
        "✓".green(),
        out_path.display(),
        format_size(bytes.len()),
    );
    Ok(())
}

// ─── browse ────────────────────────────────────────────────────

/// 交互式浏览课程内容
pub async fn cmd_browse(course_id: Option<&str>) -> Result<()> {
    let api = CourseApi::from_session()?;

    // Step 1: 选择课程
    let course = match course_id {
        Some(id) => resolve_course_id(&api, id).await?,
        None => {
            let courses = api.list_courses(false).await?;
            if courses.is_empty() {
                println!("{}", "暂无课程".dimmed());
                return Ok(());
            }
            display::print_courses(&courses);
            println!("请输入课程编号:");
            let idx = read_index()?.saturating_sub(1);
            courses
                .get(idx)
                .ok_or_else(|| anyhow!("无效的课程编号"))?
                .id
                .clone()
        }
    };

    // Step 2: 显示侧边栏
    let entries = api.list_course_entries(&course).await?;
    if entries.is_empty() {
        println!("{}", "课程无内容".dimmed());
        return Ok(());
    }

    display::print_course_entries(&course, &entries);

    // Step 3: 选择侧边栏入口
    println!("请输入入口编号 (输入 q 退出):");
    loop {
        let input = read_line()?;
        if input == "q" || input == "quit" {
            break;
        }
        let idx: usize = input.parse::<usize>().unwrap_or(0).saturating_sub(1);
        if let Some(entry) = entries.get(idx) {
            if let Some((content_id, _)) = CourseApi::parse_content_url(&entry.url) {
                let items = api.list_content(&course, &content_id).await?;
                display::print_content_list(&items);
            } else {
                println!(
                    "  {} 该入口不是内容列表页面: {}",
                    "[info]".yellow(),
                    entry.url.dimmed()
                );
            }
        } else {
            println!("{}", "无效编号".red());
        }
        println!("请输入入口编号 (输入 q 退出):");
    }

    Ok(())
}

// ─── helpers ───────────────────────────────────────────────────

/// 如果 course_id 是数字，从课程列表中按索引解析
async fn resolve_course_id(api: &CourseApi, input: &str) -> Result<String> {
    if input.starts_with('_') && input.ends_with("_1") {
        // 已经是 Blackboard 内部 ID 格式
        return Ok(input.to_string());
    }

    if let Ok(idx) = input.parse::<usize>() {
        let courses = api.list_courses(false).await?;
        let course = courses
            .get(idx.saturating_sub(1))
            .ok_or_else(|| anyhow!("无效的课程编号 {idx}，共 {} 门课程", courses.len()))?;
        return Ok(course.id.clone());
    }

    // 尝试作为课程名搜索
    let courses = api.list_courses(false).await?;
    let matched = courses
        .iter()
        .find(|c| c.name().contains(input) || c.long_title.contains(input));

    match matched {
        Some(c) => Ok(c.id.clone()),
        None => Err(anyhow!(
            "未找到匹配的课程 \"{input}\"。使用 `course courses` 查看课程列表"
        )),
    }
}

fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn read_line() -> Result<String> {
    use std::io::{self, Write};
    print!("> ");
    io::stdout().flush()?;
    let mut buf = String::new();
    io::stdin().read_line(&mut buf)?;
    Ok(buf.trim().to_string())
}

fn read_index() -> Result<usize> {
    let line = read_line()?;
    line.parse::<usize>()
        .map_err(|_| anyhow!("请输入有效数字"))
}
