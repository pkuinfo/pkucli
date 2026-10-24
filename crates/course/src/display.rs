//! 终端格式化输出

use crate::api::{
    self, Announcement, AnnouncementSummary, AssignmentDetail, AssignmentSummary, ContentItem,
    ContentType, CourseEntry, CourseInfo, VideoInfo,
};
use crate::ta;
use colored::Colorize;
use std::collections::HashMap;

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

// ─── 助教 ──────────────────────────────────────────────────────

/// 内容类型的中文名
pub fn kind_label(handler: &str) -> &str {
    match handler {
        ta::FOLDER => "文件夹",
        ta::ASSIGNMENT => "作业",
        "resource/x-bb-file" => "文件",
        "resource/x-bb-document" => "项目",
        "resource/x-bb-externallink" => "链接",
        other => other.strip_prefix("resource/x-bb-").unwrap_or(other),
    }
}

/// 提交状态的中文名
pub fn status_label(status: Option<&str>) -> colored::ColoredString {
    match status {
        Some("NeedsGrading") => "待评分".yellow(),
        Some("Completed") => "已评分".green(),
        Some("InProgress") => "未提交完".dimmed(),
        Some(s) => s.normal(),
        None => "未提交".dimmed(),
    }
}

fn fmt_utc(s: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|_| s.to_string())
}

/// 打印内容区/文件夹里的条目（含隐藏条目）
pub fn print_ta_contents(title: &str, id: &str, items: &[ta::Content]) {
    println!("{} {}", title.bold(), id.dimmed());
    if items.is_empty() {
        println!("  {}", "（空）".dimmed());
    }
    for c in items {
        let hidden = if c.is_visible() {
            String::new()
        } else {
            format!(" {}", "[对学生隐藏]".yellow())
        };
        println!(
            "  {}  {:<4} {}{}",
            c.id.dimmed(),
            kind_label(c.handler()),
            c.title,
            hidden
        );
    }
}

/// 打印成绩中心的列
pub fn print_ta_columns(columns: &[ta::Column]) {
    for c in columns {
        let kind = match c.kind() {
            "Calculated" => "计算列·只读".red(),
            "Attempts" => "作业".cyan(),
            "Manual" => "手动".normal(),
            other => other.normal(),
        };
        let possible = c
            .possible()
            .map(ta::fmt_score)
            .unwrap_or_else(|| "-".into());
        let due = c
            .grading
            .as_ref()
            .and_then(|g| g.due.as_deref())
            .map(|d| format!("  截止 {}", fmt_utc(d)))
            .unwrap_or_default();
        println!(
            "  {}  {}  [{}]  满分 {}{}",
            c.id.dimmed(),
            c.name,
            kind,
            possible,
            due.dimmed()
        );
    }
}

/// 打印学生名单
pub fn print_ta_roster(students: &[ta::Student]) {
    for s in students {
        let flag = if s.available {
            String::new()
        } else {
            format!(" {}", "[已停用]".dimmed())
        };
        println!("  {}  {}  {}{}", s.user_name, s.name, s.dept.dimmed(), flag);
    }
    println!(
        "\n共 {} 名学生（已排除预览账号）",
        students.len().to_string().bold()
    );
}

/// 打印某次作业每个学生的最新提交状态
pub fn print_ta_attempts(
    col: &ta::Column,
    students: &[ta::Student],
    by_user: &HashMap<String, Vec<ta::Attempt>>,
    needs_grading: bool,
) {
    println!("{} {}", col.name.bold(), col.id.dimmed());
    let (mut submitted, mut pending) = (0, 0);
    for s in students {
        let list = by_user.get(&s.user_id);
        let latest = list.and_then(|l| l.last());
        let status = latest.and_then(|a| a.status.as_deref());
        if latest.is_some() {
            submitted += 1;
        }
        if status == Some("NeedsGrading") {
            pending += 1;
        }
        if needs_grading && status != Some("NeedsGrading") {
            continue;
        }
        let score = latest
            .and_then(|a| a.score)
            .map(ta::fmt_score)
            .unwrap_or_default();
        let times = match list.map_or(0, Vec::len) {
            0 | 1 => String::new(),
            n => format!(" ({n} 次)"),
        };
        println!(
            "  {}  {:<6} {} {}{}",
            s.user_name,
            s.name,
            status_label(status),
            score.bold(),
            times.dimmed()
        );
    }
    println!(
        "\n已提交 {}/{}，待评分 {}",
        submitted,
        students.len(),
        pending.to_string().yellow()
    );
}

/// 打印课堂实录列表（序号即 publish/unpublish 用的序号）
pub fn print_ta_recordings(list: &[ta::Recording]) {
    if list.is_empty() {
        println!("{}", "暂无课堂实录".dimmed());
        return;
    }
    let mut unpublished = 0;
    for (i, r) in list.iter().enumerate() {
        let state = if r.published {
            "已发布".green()
        } else {
            unpublished += 1;
            "未发布".yellow()
        };
        println!(
            "  {:>2}. {}  {}  {}  {}",
            i + 1,
            r.time,
            r.title,
            state,
            r.sub_id.dimmed()
        );
    }
    println!(
        "\n共 {} 节，未发布 {}",
        list.len(),
        unpublished.to_string().yellow()
    );
}
