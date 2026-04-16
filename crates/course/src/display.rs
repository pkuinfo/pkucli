//! 终端格式化输出

use crate::api::{
    self, Announcement, AnnouncementSummary, AssignmentDetail, AssignmentSummary, ContentItem,
    ContentType, CourseEntry, CourseInfo, VideoInfo,
};
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
        format!(
            "共 {} 门课程（当前 {} / 往期 {}）",
            courses.len(),
            current.len(),
            past.len()
        )
        .dimmed()
    );
}

/// 打印课程侧边栏入口
pub fn print_course_entries(course_name: &str, entries: &[CourseEntry]) {
    println!("{}", format!("── {} ──", course_name).bold());
    println!();
    for (i, entry) in entries.iter().enumerate() {
        println!("  {} {}", format!("[{}]", i + 1).cyan(), entry.name.bold(),);
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
            println!(
                "      {} {} {}",
                "附件".dimmed(),
                att.name,
                att.url.dimmed()
            );
        }
    }
    println!();
}

/// 打印作业详情
pub fn print_assignment_detail(detail: &AssignmentDetail) {
    println!("{}", format!("── {} ──", detail.title).bold());
    println!();

    if let Some(deadline) = &detail.deadline {
        print!("  {} {}", "截止时间:".yellow(), deadline);
        // 尝试解析并显示倒计时
        if let Some(dt) = api::parse_deadline(deadline) {
            let delta = dt - chrono::Local::now();
            print!("  ({})", api::fmt_time_delta(delta));
        }
        println!();
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
            println!("    附件 {}", att.name);
        }
    }
    println!();
}

/// 打印跨课程作业汇总列表
pub fn print_assignments_list(assignments: &[AssignmentSummary], show_all: bool) {
    let title = if show_all {
        "所有作业 (包括已完成)"
    } else {
        "未完成作业"
    };
    println!(
        "{}\n",
        format!("── {} ({}) ──", title, assignments.len()).bold()
    );

    for (i, a) in assignments.iter().enumerate() {
        // 课程名 > 作业标题
        print!(
            "  {} {} {} {} ",
            format!("[{}]", i + 1).cyan(),
            a.course_name.bold().blue(),
            ">".dimmed(),
            a.title.bold(),
        );

        // 提交状态 / 截止时间
        if let Some(attempt) = &a.last_attempt {
            print!("({})", format!("已完成: {attempt}").green());
        } else if let Some(dt) = a.deadline {
            let delta = dt - chrono::Local::now();
            print!("({})", api::fmt_time_delta(delta));
        } else if let Some(raw) = &a.deadline_raw {
            print!("({})", raw.dimmed());
        } else {
            print!("({})", "无截止时间".dimmed());
        }

        // hash ID
        println!(" {}", a.hash_id.dimmed());

        // 描述
        for desc in &a.descriptions {
            if !desc.is_empty() {
                println!("      {}", truncate_display(desc, 80).dimmed());
            }
        }

        // 附件
        for att in &a.attachments {
            println!("      {} {}", "附件".dimmed(), att.name);
        }
    }
    println!();
}

/// 打印课程回放列表
pub fn print_videos(videos: &[VideoInfo]) {
    println!("{}", "── 课程回放 ──".bold());
    println!();

    // 按课程分组
    let mut current_course = String::new();
    for (i, v) in videos.iter().enumerate() {
        if v.course_name != current_course {
            if !current_course.is_empty() {
                println!();
            }
            println!("{}", format!("  [{}]", v.course_name).bold().blue());
            current_course = v.course_name.clone();
        }
        println!(
            "    {} {} {} {}",
            format!("[{}]", i + 1).cyan(),
            v.title,
            format!("({})", v.time).dimmed(),
            v.hash_id.dimmed(),
        );
    }
    println!();
    println!("{}", format!("共 {} 个回放", videos.len()).dimmed());
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

/// 打印单条公告
pub fn print_announcement(ann: &Announcement) {
    println!("  ──────────────────────────────────────────────────────────────");
    println!("  {}", ann.title.bold());
    if !ann.date.is_empty() {
        println!("  {}", ann.date.dimmed());
    }
    if !ann.author.is_empty() {
        println!("  {}", format!("发布者: {}", ann.author).dimmed());
    }
    if !ann.body.is_empty() {
        let body = truncate_display(&ann.body, 200);
        println!("  {}", body);
    }
}

/// 打印跨课程公告汇总
pub fn print_announcement_summary(summary: &AnnouncementSummary) {
    let ann = &summary.announcement;
    println!("  ──────────────────────────────────────────────────────────────");
    println!(
        "  {} {}",
        format!("[{}]", summary.course_name).cyan(),
        ann.title.bold(),
    );
    if !ann.date.is_empty() {
        println!("  {}", ann.date.dimmed());
    }
    if !ann.author.is_empty() {
        println!("  {}", format!("发布者: {}", ann.author).dimmed());
    }
    if !ann.body.is_empty() {
        let body = truncate_display(&ann.body, 200);
        println!("  {}", body);
    }
}
