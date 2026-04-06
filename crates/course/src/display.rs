//! 终端格式化输出

use crate::api::{AssignmentDetail, ContentItem, ContentType, CourseEntry, CourseInfo};
use colored::Colorize;

/// 打印课程列表
pub fn print_courses(courses: &[CourseInfo]) {
    let current: Vec<_> = courses.iter().filter(|c| c.is_current).collect();
    let past: Vec<_> = courses.iter().filter(|c| !c.is_current).collect();

    if !current.is_empty() {
        println!("{}", "── 当前学期课程 ──".bold());
        println!();
        for (i, c) in current.iter().enumerate() {
            println!(
                "  {} {} {}",
                format!("[{}]", i + 1).cyan(),
                c.name().bold(),
                format!("({})", c.id).dimmed(),
            );
            // 如果有学期后缀，显示
            let title = c.title();
            if title != c.name() {
                if let Some(semester) = title.rfind('(').map(|i| &title[i..]) {
                    println!("      {}", semester.dimmed());
                }
            }
        }
        println!();
    }

    if !past.is_empty() {
        println!("{}", "── 往期课程 ──".bold());
        println!();
        for (i, c) in past.iter().enumerate() {
            let idx = current.len() + i + 1;
            println!(
                "  {} {} {}",
                format!("[{}]", idx).dimmed(),
                c.name(),
                format!("({})", c.id).dimmed(),
            );
        }
        println!();
    }

    println!(
        "{}",
        format!("共 {} 门课程（当前 {} / 往期 {}）", courses.len(), current.len(), past.len())
            .dimmed()
    );
}

/// 打印课程侧边栏入口
pub fn print_course_entries(course_name: &str, entries: &[CourseEntry]) {
    println!("{}", format!("── {} ──", course_name).bold());
    println!();
    for (i, entry) in entries.iter().enumerate() {
        println!(
            "  {} {}",
            format!("[{}]", i + 1).cyan(),
            entry.name.bold(),
        );
    }
    println!();
}

/// 打印内容列表
pub fn print_content_list(items: &[ContentItem]) {
    if items.is_empty() {
        println!("{}", "  暂无内容".dimmed());
        return;
    }

    for (i, item) in items.iter().enumerate() {
        let type_badge = match &item.item_type {
            ContentType::Assignment => "[作业]".red().bold().to_string(),
            ContentType::Folder => "[文件夹]".blue().to_string(),
            ContentType::Document => "[文件]".green().to_string(),
        };

        println!(
            "  {} {} {}",
            format!("[{}]", i + 1).cyan(),
            type_badge,
            item.title.bold(),
        );

        if !item.description.is_empty() {
            let desc = truncate_display(&item.description, 80);
            println!("      {}", desc.dimmed());
        }

        if let Some(url) = &item.url {
            println!("      {}", url.dimmed());
        }

        for att in &item.attachments {
            println!("      {} {} {}", "📎".dimmed(), att.name, att.url.dimmed());
        }
    }
    println!();
}

/// 打印作业详情
pub fn print_assignment_detail(detail: &AssignmentDetail) {
    println!("{}", format!("── {} ──", detail.title).bold());
    println!();

    if let Some(deadline) = &detail.deadline {
        println!("  {} {}", "截止时间:".yellow(), deadline);
    }

    println!("  {} {}", "提交状态:".cyan(), detail.status);

    if !detail.instructions.is_empty() {
        println!();
        println!("  {}", "说明:".bold());
        for line in detail.instructions.lines() {
            println!("    {}", line);
        }
    }

    if !detail.attachments.is_empty() {
        println!();
        println!("  {} ({}个)", "附件:".bold(), detail.attachments.len());
        for att in &detail.attachments {
            println!("    📎 {}", att.name);
        }
    }
    println!();
}

/// 截断文本到指定宽度（中文算 2 宽度）
fn truncate_display(s: &str, max_width: usize) -> String {
    let mut width = 0;
    let mut result = String::new();
    for ch in s.chars() {
        let w = if ch.is_ascii() { 1 } else { 2 };
        if width + w > max_width {
            result.push_str("...");
            break;
        }
        if ch == '\n' {
            result.push(' ');
            width += 1;
        } else {
            result.push(ch);
            width += w;
        }
    }
    result
}
