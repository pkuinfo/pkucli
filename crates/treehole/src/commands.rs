//! 所有子命令的实现

use crate::api::{CreateCommentReq, CreateHoleReq, TreeholeApi};
use crate::display;
use anyhow::{anyhow, Result};
use chrono::Datelike;
use colored::Colorize;
use std::io::{self, Write};

// ─── list ───────────────────────────────────────────────────────

pub async fn cmd_list(feed: &str, page: u32, limit: u32) -> Result<()> {
    let api = TreeholeApi::from_session_verified().await?;

    let items = match feed {
        "follow" => api.list_follow(page, limit).await?,
        _ => api.list_holes(page, limit).await?,
    };

    if items.is_empty() {
        println!("{}", "暂无帖子".dimmed());
        return Ok(());
    }

    for item in &items {
        display::print_hole_item(item);
    }

    println!(
        "{}",
        format!("─── 第 {page} 页 · 共 {} 条 ───", items.len()).dimmed()
    );
    Ok(())
}

// ─── show ───────────────────────────────────────────────────────

pub async fn cmd_show(pid: i64) -> Result<()> {
    let api = TreeholeApi::from_session_verified().await?;
    let data = api.get_hole(pid).await?;
    display::print_hole_detail(&data.hole, &data.list, data.total);
    Ok(())
}

// ─── search ─────────────────────────────────────────────────────

pub async fn cmd_search(keyword: &str, page: u32, limit: u32) -> Result<()> {
    let api = TreeholeApi::from_session_verified().await?;
    let holes = api.search(keyword, page, limit).await?;

    if holes.is_empty() {
        println!("{}", "无搜索结果".dimmed());
        return Ok(());
    }

    println!(
        "{}",
        format!("── 搜索 \"{}\" ──", keyword).bold()
    );
    println!();
    for h in &holes {
        display::print_hole_simple(h);
    }
    Ok(())
}

// ─── post ───────────────────────────────────────────────────────

pub async fn cmd_post(
    text: Option<String>,
    tag: Option<String>,
    named: bool,
    fold: bool,
    reward: Option<i64>,
    images: Vec<std::path::PathBuf>,
) -> Result<()> {
    let api = TreeholeApi::from_session_verified().await?;

    let content = match text {
        Some(t) => t,
        None => {
            println!("请输入帖子内容（输入空行结束）：");
            read_multiline()?
        }
    };

    if content.trim().is_empty() {
        return Err(anyhow!("内容不能为空"));
    }

    // 上传图片
    let (post_type, media_ids) = if images.is_empty() {
        ("text".to_string(), None)
    } else {
        println!("{} 正在上传 {} 张图片...", "⏳".dimmed(), images.len());
        let ids = api.upload_images(&images).await?;
        println!("{} 图片上传完成", "✓".green());
        ("image".to_string(), Some(ids))
    };

    let req = CreateHoleReq {
        text: content,
        r#type: post_type,
        tags_ids: tag,
        anonymous: if named { 0 } else { 1 },
        fold: if fold { 1 } else { 0 },
        reward_cost: reward.unwrap_or(0),
        media_ids,
    };

    let result = api.create_hole(&req).await?;
    let pid = result
        .get("pid")
        .and_then(|v| v.as_i64())
        .map(|p| format!(" #{p}"))
        .unwrap_or_default();

    println!("{} 发帖成功{pid}", "✓".green());
    Ok(())
}

// ─── reply ──────────────────────────────────────────────────────

pub async fn cmd_reply(
    pid: i64,
    text: Option<String>,
    quote_cid: Option<i64>,
    image: Option<std::path::PathBuf>,
) -> Result<()> {
    let api = TreeholeApi::from_session_verified().await?;

    let content = match text {
        Some(t) => t,
        None => {
            // 先展示帖子上下文
            let data = api.get_hole(pid).await?;
            println!(
                "{} {}",
                format!("#{}", data.hole.pid).cyan().bold(),
                display::fmt_time(data.hole.timestamp).dimmed()
            );
            for line in data.hole.text.lines() {
                println!("  {line}");
            }
            println!();

            // 如果指定了引用评论，展示被引用的内容
            if let Some(cid) = quote_cid {
                if let Some(c) = data.list.iter().find(|c| c.cid == cid) {
                    println!(
                        "  {} [{}]: {}",
                        "引用".dimmed(),
                        c.name_tag,
                        c.text.replace('\n', " ")
                    );
                    println!();
                }
            }

            println!("请输入回复内容（输入空行结束）：");
            read_multiline()?
        }
    };

    if content.trim().is_empty() {
        return Err(anyhow!("回复内容不能为空"));
    }

    // 上传图片（评论仅限一张）
    let media_ids = if let Some(img_path) = image {
        println!("{} 正在上传图片...", "⏳".dimmed());
        let id = api.upload_image(&img_path).await?;
        println!("{} 图片上传完成", "✓".green());
        Some(id)
    } else {
        None
    };

    let req = CreateCommentReq {
        pid,
        text: content,
        comment_id: quote_cid,
        anonymous: 1,
        media_ids,
    };

    api.create_comment(&req).await?;
    println!("{} 回复成功", "✓".green());
    Ok(())
}

// ─── like / tread ───────────────────────────────────────────────

pub async fn cmd_like(pid: i64) -> Result<()> {
    let api = TreeholeApi::from_session_verified().await?;
    api.praise_hole(pid).await?;
    println!("{} 已点赞 #{pid}", "▲".green());
    Ok(())
}

pub async fn cmd_tread(pid: i64) -> Result<()> {
    let api = TreeholeApi::from_session_verified().await?;
    api.tread_hole(pid).await?;
    println!("{} 已踩 #{pid}", "▼".red());
    Ok(())
}

// ─── star (bookmark) ────────────────────────────────────────────

pub async fn cmd_star(pid: i64) -> Result<()> {
    let api = TreeholeApi::from_session_verified().await?;
    // 获取默认分组 ID
    let groups = api.list_bookmark_groups().await?;
    let bookmark_id = groups.first().map(|g| g.id);
    api.star_hole(pid, bookmark_id).await?;
    println!("{} 已收藏 #{pid}", "★".yellow());
    Ok(())
}

pub async fn cmd_unstar(pid: i64) -> Result<()> {
    let api = TreeholeApi::from_session_verified().await?;
    api.unfollow_hole(pid).await?;
    println!("{} 已取消收藏 #{pid}", "☆".dimmed());
    Ok(())
}

pub async fn cmd_stars(page: u32, limit: u32) -> Result<()> {
    let api = TreeholeApi::from_session_verified().await?;
    let items = api.list_follow(page, limit).await?;

    if items.is_empty() {
        println!("{}", "暂无收藏/关注".dimmed());
        return Ok(());
    }

    println!("{}", "── 我的收藏/关注 ──".bold());
    println!();
    for item in &items {
        display::print_hole_item(item);
    }
    Ok(())
}

// ─── follow ─────────────────────────────────────────────────────

pub async fn cmd_follow(pid: i64) -> Result<()> {
    let api = TreeholeApi::from_session_verified().await?;
    api.follow_hole(pid).await?;
    println!("{} 已关注 #{pid}", "✓".green());
    Ok(())
}

pub async fn cmd_unfollow(pid: i64) -> Result<()> {
    let api = TreeholeApi::from_session_verified().await?;
    api.unfollow_hole(pid).await?;
    println!("{} 已取消关注 #{pid}", "✓".green());
    Ok(())
}

// ─── msg ────────────────────────────────────────────────────────

pub async fn cmd_msg(page: u32, limit: u32) -> Result<()> {
    let api = TreeholeApi::from_session_verified().await?;

    let (int_count, sys_count) = api.unread_count().await?;
    if int_count > 0 || sys_count > 0 {
        println!(
            "{}",
            format!("未读：互动 {int_count} 条，系统 {sys_count} 条")
                .yellow()
        );
        println!();
    }

    let msgs = api.list_messages(page, limit).await?;
    if msgs.is_empty() {
        println!("{}", "暂无消息".dimmed());
        return Ok(());
    }

    for m in &msgs {
        display::print_message(m);
    }
    Ok(())
}

pub async fn cmd_msg_read(ids: Vec<i64>) -> Result<()> {
    let api = TreeholeApi::from_session_verified().await?;
    api.mark_read(&ids).await?;
    println!("{} 已标记 {} 条消息为已读", "✓".green(), ids.len());
    Ok(())
}

// ─── me ─────────────────────────────────────────────────────────

pub async fn cmd_me(show_posts: bool, page: u32, limit: u32) -> Result<()> {
    let api = TreeholeApi::from_session_verified().await?;
    let info = api.user_info().await?;
    display::print_user_info(&info);

    if show_posts {
        println!();
        println!("{}", "── 我的帖子 ──".bold());
        println!();
        let holes = api.my_holes(page, limit).await?;
        if holes.is_empty() {
            println!("{}", "暂无帖子".dimmed());
        } else {
            for h in &holes {
                display::print_hole_simple(h);
            }
        }
    }
    Ok(())
}

// ─── report ─────────────────────────────────────────────────────

pub async fn cmd_report(pid: i64, reason: &str) -> Result<()> {
    let api = TreeholeApi::from_session_verified().await?;
    api.report_hole(pid, reason).await?;
    println!("{} 已举报 #{pid}", "✓".green());
    Ok(())
}

// ─── score ──────────────────────────────────────────────────────

pub async fn cmd_score(semester: Option<&str>, no_color: bool) -> Result<()> {
    let api = TreeholeApi::from_session_verified().await?;
    let data = api.get_scores().await?;
    display::print_scores(&data, semester, no_color);
    Ok(())
}

// ─── course ─────────────────────────────────────────────────────

pub async fn cmd_course(show_times: bool) -> Result<()> {
    let api = TreeholeApi::from_session_verified().await?;

    if show_times {
        let times = api.get_class_times().await?;
        display::print_class_times(&times);
        println!();
    }

    let rows = api.get_coursetable().await?;
    display::print_coursetable(&rows);
    Ok(())
}

// ─── academic calendar ──────────────────────────────────────────

pub async fn cmd_academic_cal(start: Option<&str>, end: Option<&str>) -> Result<()> {
    let api = TreeholeApi::from_session_verified().await?;

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let default_end = (chrono::Local::now() + chrono::Duration::days(30))
        .format("%Y-%m-%d")
        .to_string();
    let start = start.unwrap_or(&today);
    let end = end.unwrap_or(&default_end);

    let events = api.list_lab_events(start, end).await?;
    if events.is_empty() {
        println!("{}", "暂无学术日历事件".dimmed());
        return Ok(());
    }

    println!(
        "{}",
        format!("── 学术日历 ({start} ~ {end}) ──").bold()
    );
    println!();
    for e in &events {
        display::print_lab_event(e);
    }
    println!(
        "{}",
        format!("共 {} 场学术活动", events.len()).dimmed()
    );
    Ok(())
}

// ─── activity calendar ──────────────────────────────────────────

pub async fn cmd_activity_cal(
    start: Option<&str>,
    end: Option<&str>,
    page: u32,
    limit: u32,
) -> Result<()> {
    let api = TreeholeApi::from_session_verified().await?;

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let default_end = (chrono::Local::now() + chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let start = start.unwrap_or(&today);
    let end = end.unwrap_or(&default_end);

    let events = api.list_activity_events(start, end, page, limit).await?;
    if events.is_empty() {
        println!("{}", "暂无活动".dimmed());
        return Ok(());
    }

    println!(
        "{}",
        format!("── 活动日历 ({start} ~ {end}) ──").bold()
    );
    println!();
    for e in &events {
        display::print_activity_event(e);
    }
    Ok(())
}

// ─── schedule ───────────────────────────────────────────────────

pub async fn cmd_schedule(start: Option<&str>) -> Result<()> {
    let api = TreeholeApi::from_session_verified().await?;

    let now = chrono::Local::now();
    // Default to this Monday
    let weekday = now.weekday().num_days_from_monday();
    let monday = now - chrono::Duration::days(weekday as i64);
    let sunday = monday + chrono::Duration::days(7);

    let start_str = start
        .map(|s| s.to_string())
        .unwrap_or_else(|| monday.format("%Y-%m-%d").to_string());
    let end_str = if let Some(s) = start {
        let d = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .unwrap_or(monday.date_naive());
        (d + chrono::Duration::days(7))
            .format("%Y-%m-%d")
            .to_string()
    } else {
        sunday.format("%Y-%m-%d").to_string()
    };

    let items = api.list_schedules(&start_str, &end_str).await?;
    if items.is_empty() {
        println!("{}", "本周暂无日程".dimmed());
        return Ok(());
    }

    println!(
        "{}",
        format!("── 日程 ({start_str} ~ {end_str}) ──").bold()
    );
    println!();
    for s in &items {
        display::print_schedule_item(s);
    }
    Ok(())
}

// ─── helpers ────────────────────────────────────────────────────

fn read_multiline() -> Result<String> {
    let mut lines = Vec::new();
    loop {
        let mut buf = String::new();
        io::stdout().flush()?;
        io::stdin().read_line(&mut buf)?;
        if buf.trim().is_empty() && !lines.is_empty() {
            break;
        }
        lines.push(buf.trim_end_matches('\n').to_string());
    }
    Ok(lines.join("\n"))
}
