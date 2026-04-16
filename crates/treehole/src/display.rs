//! 终端格式化输出

use crate::api::{
    ActivityEvent, ClassTime, Comment, CourseRow, Hole, HoleListItem, LabEvent, Message,
    ScheduleItem, ScoreData, UserInfo,
};
use crate::colorize;
use colored::Colorize;

/// 格式化时间戳为人类可读
pub fn fmt_time(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| {
            let local = dt.with_timezone(&chrono::Local);
            let now = chrono::Local::now();
            let diff = now.signed_duration_since(local);

            let relative = if diff.num_seconds() < 60 {
                "刚刚".to_string()
            } else if diff.num_minutes() < 60 {
                format!("{}分钟前", diff.num_minutes())
            } else if diff.num_hours() < 24 {
                format!("{}小时前", diff.num_hours())
            } else if diff.num_days() < 7 {
                format!("{}天前", diff.num_days())
            } else {
                local.format("%m-%d %H:%M").to_string()
            };
            relative
        })
        .unwrap_or_else(|| "?".to_string())
}

/// 截断文本到指定宽度（按字符计数，中文算 2 宽度）
fn truncate_display(s: &str, max_width: usize) -> String {
    let mut width = 0;
    let mut result = String::new();
    for ch in s.chars() {
        let w = if ch.is_ascii() { 1 } else { 2 };
        if width + w > max_width {
            result.push_str("...");
            break;
        }
        result.push(ch);
        width += w;
    }
    result
}

/// 图片数量提示
fn media_badge(media_ids: &str) -> String {
    if media_ids.is_empty() {
        return String::new();
    }
    let count = media_ids.split(',').filter(|s| !s.is_empty()).count();
    if count == 1 {
        format!(" {}", "[图片]".magenta())
    } else {
        format!(" {}", format!("[{count}张图片]").magenta())
    }
}

/// 打印帖子列表中的一项
pub fn print_hole_item(item: &HoleListItem) {
    print_hole_item_header(item);

    // 正文预览
    let text = item.text.replace('\n', " ");
    let preview = truncate_display(&text, 80);
    print!("  {}", preview);
    println!("{}", media_badge(&item.media_ids));

    // 评论预览
    for c in item.comment_list.iter().take(3) {
        let tag = format!("[{}]", c.name_tag);
        let ctext = c.text.replace('\n', " ");
        let cpreview = truncate_display(&ctext, 60);
        println!("    {} {}", tag.dimmed(), cpreview.dimmed());
    }
    if item.reply > 3 {
        println!("    {}", format!("... 共 {} 条评论", item.reply).dimmed());
    }
    println!();
}

fn print_hole_item_header(h: &HoleListItem) {
    let pid = format!("#{}", h.pid).bold().cyan();
    let time = fmt_time(h.timestamp).dimmed();

    let mut badges = Vec::new();
    if h.is_top == 1 {
        badges.push("[置顶]".red().to_string());
    }
    if h.reward_cost > 0 {
        badges.push(format!("[悬赏{}🍃]", h.reward_cost).yellow().to_string());
    }
    for t in &h.tags_info {
        badges.push(format!("#{}", t.tag_name).blue().to_string());
    }
    if h.is_follow == 1 {
        badges.push("[关注中]".green().to_string());
    }

    let badge_str = if badges.is_empty() {
        String::new()
    } else {
        format!(" {}", badges.join(" "))
    };

    let stats = format!(
        "{}{}{}",
        format!(" ▲{}", h.likenum).green(),
        format!(" ▼{}", h.tread_num).red(),
        format!(" 💬{}", h.reply).dimmed(),
    );

    println!("{}{} {}{}", pid, badge_str, time, stats);
}

/// 打印帖子头部（PID + 标签 + 时间 + 互动数据）
fn print_hole_header(h: &Hole) {
    let pid = format!("#{}", h.pid).bold().cyan();
    let time = fmt_time(h.timestamp).dimmed();

    let mut badges = Vec::new();
    if h.is_top == 1 {
        badges.push("[置顶]".red().to_string());
    }
    if h.reward_cost > 0 {
        badges.push(format!("[悬赏{}🍃]", h.reward_cost).yellow().to_string());
    }
    for t in &h.tags_info {
        badges.push(format!("#{}", t.tag_name).blue().to_string());
    }
    if h.is_follow == 1 {
        badges.push("[关注中]".green().to_string());
    }

    let badge_str = if badges.is_empty() {
        String::new()
    } else {
        format!(" {}", badges.join(" "))
    };

    let stats = format!(
        "{}{}{}",
        format!(" ▲{}", h.likenum).green(),
        format!(" ▼{}", h.tread_num).red(),
        format!(" 💬{}", h.reply).dimmed(),
    );

    println!("{}{} {}{}", pid, badge_str, time, stats);
}

/// 打印帖子详情（完整正文 + 全部评论）
pub fn print_hole_detail(h: &Hole, comments: &[Comment], total: Option<i64>) {
    print_hole_header(h);
    println!();

    // 完整正文
    for line in h.text.lines() {
        println!("  {line}");
    }
    if !h.media_ids.is_empty() {
        println!("  {}", media_badge(&h.media_ids).trim());
    }
    println!();

    // 分割线
    let total_count = total.unwrap_or(comments.len() as i64);
    println!(
        "{}",
        format!("── 评论 ({total_count}) ──────────────────────").dimmed()
    );
    println!();

    for c in comments {
        print_comment(c);
    }
}

/// 打印单条评论
pub fn print_comment(c: &Comment) {
    let tag_str = if c.is_lz == 1 {
        format!("[{}]", c.name_tag).bold().yellow().to_string()
    } else {
        format!("[{}]", c.name_tag).bold().to_string()
    };

    let cid = format!("#{}", c.cid).dimmed();
    let time = fmt_time(c.timestamp).dimmed();

    // 引用
    if let Some(q) = c.quote.as_object() {
        if let (Some(qt), Some(qn)) = (
            q.get("text").and_then(|v| v.as_str()),
            q.get("name_tag").and_then(|v| v.as_str()),
        ) {
            let qtext = truncate_display(&qt.replace('\n', " "), 50);
            println!("  {} {}", format!("Re {qn}:").dimmed(), qtext.dimmed());
        }
    }

    println!("  {} {} {}", tag_str, cid, time);

    for line in c.text.lines() {
        println!("    {line}");
    }
    if !c.media_ids.is_empty() {
        println!("    {}", "[图片]".magenta());
    }
    println!();
}

/// 打印简单帖子列表（搜索/我的帖子）
pub fn print_hole_simple(h: &Hole) {
    print_hole_header(h);
    let text = h.text.replace('\n', " ");
    let preview = truncate_display(&text, 80);
    print!("  {}", preview);
    println!("{}", media_badge(&h.media_ids));
    println!();
}

/// 打印消息
pub fn print_message(m: &Message) {
    let read_mark = if m.is_read == 0 {
        "●".red().to_string()
    } else {
        "○".dimmed().to_string()
    };

    let pid_str = m
        .pid
        .map(|p| format!(" #{p}").cyan().to_string())
        .unwrap_or_default();

    println!(
        "{} {}{} {}",
        read_mark,
        m.title.bold(),
        pid_str,
        m.created_at.dimmed()
    );
    if !m.content.is_empty() {
        let preview = truncate_display(&m.content.replace('\n', " "), 70);
        println!("  {}", preview.dimmed());
    }
    println!();
}

/// 打印用户信息
pub fn print_user_info(u: &UserInfo) {
    println!("{}", "── 个人信息 ──".bold());
    println!("  UID          {}", u.uid);
    println!("  姓名         {}", u.name);
    println!("  剩余操作次数  {}", u.action_remaining);
    println!("  树叶余额      {}", u.leaf_balance);
    println!("  未读消息      {}", u.newmsgcount);
    if u.is_black == 1 {
        println!("  {}", "⚠ 账号处于黑名单状态".red());
    }
}

// ─── 成绩 ───────────────────────────────────────────────────────

/// 格式化学期名 "25-26-1" → "25-26学年度1学期"
fn fmt_semester(xndxq: &str) -> String {
    let parts: Vec<&str> = xndxq.split('-').collect();
    if parts.len() == 3 {
        format!("{}-{}学年度{}学期", parts[0], parts[1], parts[2])
    } else {
        xndxq.to_string()
    }
}

pub fn print_scores(data: &ScoreData, semester_filter: Option<&str>, no_color: bool) {
    println!(
        "{}  总学分: {}  总GPA: {}",
        "── 成绩查询 ──".bold(),
        data.total_credits.bold(),
        if no_color {
            data.overall_gpa.bold().to_string()
        } else {
            colorize::colorize_gpa(&data.overall_gpa).to_string()
        }
    );
    println!();

    // Group courses by semester (xnd + xq → "25-26-1")
    let mut semesters: Vec<&str> = data
        .semester_gpas
        .iter()
        .map(|g| g.xndxq.as_str())
        .collect();
    // If no semester GPAs, derive from courses
    if semesters.is_empty() {
        let mut seen = std::collections::HashSet::new();
        for c in &data.courses {
            let key = format!("{}-{}", c.xnd, c.xq);
            if seen.insert(key.clone()) {
                semesters.push(Box::leak(key.into_boxed_str()));
            }
        }
    }

    for sem_key in &semesters {
        // Apply filter
        if let Some(filter) = semester_filter {
            if !sem_key.contains(filter) {
                continue;
            }
        }

        let sem_gpa = data
            .semester_gpas
            .iter()
            .find(|g| g.xndxq == *sem_key)
            .map(|g| g.gpa.as_str())
            .unwrap_or("N/A");

        let sem_name = fmt_semester(sem_key);
        let gpa_display = if no_color {
            sem_gpa.bold().to_string()
        } else {
            colorize::colorize_gpa(sem_gpa).to_string()
        };

        println!("  {} {}", sem_name.bold(), gpa_display);
        println!();

        // Filter courses for this semester
        let sem_courses: Vec<_> = data
            .courses
            .iter()
            .filter(|c| {
                let key = format!("{}-{}", c.xnd, c.xq);
                key == *sem_key
            })
            .collect();

        for c in &sem_courses {
            let credits = format!("{}学分", c.xf);
            let category = &c.kclbmc;
            let score_display = if no_color {
                c.xqcj.bold().to_string()
            } else {
                colorize::colorize_score(&c.xqcj).to_string()
            };
            let bar = if no_color {
                String::new()
            } else {
                format!(" {}", colorize::score_bar(&c.xqcj, 10))
            };

            println!(
                "    {} {} {} {}{}",
                credits.dimmed(),
                category.dimmed(),
                c.kcmc,
                score_display,
                bar,
            );
        }
        println!();
    }
}

// ─── 课表 ───────────────────────────────────────────────────────

pub fn print_coursetable(rows: &[CourseRow]) {
    let days = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];

    println!(
        "  {:>8}  {}",
        "节次".bold(),
        days.iter()
            .map(|d| format!("{:^14}", d.bold()))
            .collect::<Vec<_>>()
            .join("")
    );
    println!("  {}", "─".repeat(8 + 14 * 7));

    for row in rows {
        // Check if any slot is non-empty
        let has_content = row.slots.iter().any(|s| s.is_some());
        if !has_content {
            continue;
        }

        let time_label = format!("{:>8}", row.time_num);
        let slots_str: Vec<String> = row
            .slots
            .iter()
            .map(|slot| match slot {
                Some(s) => {
                    let name = truncate_display(&s.course_name, 12);
                    // Color based on the CSS background-color
                    let colored_name = match s.style.as_str() {
                        s if s.contains("aquamarine") => name.on_cyan().black().to_string(),
                        s if s.contains("lightcoral") => name.on_red().white().to_string(),
                        s if s.contains("lightcyan") => name.on_bright_cyan().black().to_string(),
                        s if s.contains("lightpink") => name.on_magenta().white().to_string(),
                        s if s.contains("lightgrey") | s.contains("lightgray") => {
                            name.on_white().black().to_string()
                        }
                        s if s.contains("lightgreen") => name.on_green().black().to_string(),
                        s if s.contains("lightsalmon") => name.on_yellow().black().to_string(),
                        s if s.contains("lightseagreen") => {
                            name.on_bright_green().black().to_string()
                        }
                        _ => name.to_string(),
                    };
                    format!("{:^14}", colored_name)
                }
                None => format!("{:^14}", "·".dimmed()),
            })
            .collect();

        println!("  {}  {}", time_label.dimmed(), slots_str.join(""));
    }
    println!();
}

pub fn print_class_times(times: &[ClassTime]) {
    println!("{}", "── 作息时间 ──".bold());
    for t in times {
        println!("  {:>8}  {}", t.name.bold(), t.time_period.dimmed());
    }
}

// ─── 学术日历 ───────────────────────────────────────────────────

pub fn print_lab_event(e: &LabEvent) {
    let date = format!("📅 {}", e.start_time).cyan();
    println!("  {}", date);
    println!("    📌 {}", e.title.bold());
    println!(
        "    {}  {}",
        e.dept.blue(),
        e.host.as_deref().unwrap_or("").dimmed()
    );
    if let Some(loc) = &e.location {
        if !loc.is_empty() {
            println!("    📍 {}", loc.dimmed());
        }
    }
    println!();
}

// ─── 活动日历 ───────────────────────────────────────────────────

pub fn print_activity_event(e: &ActivityEvent) {
    let tag = format!("[{}]", e.event_type_name).blue();
    println!("  {} {}", tag, e.event_name.bold());
    println!("    📅 {}", e.event_start_time.cyan());
    if !e.event_location.is_empty() {
        println!("    📍 {}", e.event_location.dimmed());
    }
    if !e.event_organizer.is_empty() {
        println!("    🏫 {}", e.event_organizer.dimmed());
    }
    if !e.event_introduction.is_empty() {
        let preview = truncate_display(&e.event_introduction, 80);
        println!("    {}", preview.dimmed());
    }
    println!();
}

// ─── 日程 ───────────────────────────────────────────────────────

pub fn print_schedule_item(s: &ScheduleItem) {
    println!(
        "  {} {} ~ {}",
        s.title.bold(),
        s.start_time.cyan(),
        s.end_time.cyan()
    );
    if !s.content.is_empty() {
        println!("    {}", s.content.dimmed());
    }
    println!();
}
