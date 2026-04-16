//! 所有子命令的实现

use crate::api::{self, CourseApi};
use crate::display;
use anyhow::{anyhow, Context, Result};
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

// ─── assignment (single) ──────────────────────────────────────

pub async fn cmd_assignment(course_id: &str, content_id: &str) -> Result<()> {
    let api = CourseApi::from_session()?;
    let actual_course = resolve_course_id(&api, course_id).await?;

    let detail = api.get_assignment(&actual_course, content_id).await?;
    display::print_assignment_detail(&detail);

    if let Some(attempt) = api
        .get_assignment_attempt(&actual_course, content_id)
        .await?
    {
        println!("  {} {}", "最近提交:".green(), attempt);
        println!();
    }
    Ok(())
}

// ─── assignments (全部列出) ───────────────────────────────────

pub async fn cmd_assignments(all: bool, all_term: bool) -> Result<()> {
    let api = CourseApi::from_session()?;
    let only_current = !all_term;
    let courses = api.list_courses(only_current).await?;

    if courses.is_empty() {
        println!("{}", "暂无课程".dimmed());
        return Ok(());
    }

    let pb = indicatif::ProgressBar::new(courses.len() as u64);
    pb.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("{prefix} [{bar:30}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=> "),
    );
    pb.set_prefix("扫描课程");

    let mut all_assignments = Vec::new();
    for course in &courses {
        pb.set_message(course.name().to_string());
        match api.list_assignments_for_course(course).await {
            Ok(assignments) => all_assignments.extend(assignments),
            Err(e) => {
                tracing::warn!("获取 {} 的作业失败: {e:#}", course.name());
            }
        }
        pb.inc(1);
    }
    pb.finish_and_clear();

    // 过滤：默认只显示未完成
    if !all {
        all_assignments.retain(|a| a.last_attempt.is_none());
    }

    // 按截止时间排序
    all_assignments.sort_by_key(|a| a.deadline);

    if all_assignments.is_empty() {
        let msg = if all {
            "暂无作业"
        } else {
            "暂无未完成作业"
        };
        println!("{}", msg.dimmed());
        return Ok(());
    }

    display::print_assignments_list(&all_assignments, all);
    Ok(())
}

// ─── assignment-download ──────────────────────────────────────

pub async fn cmd_assignment_download(
    id: Option<&str>,
    output_dir: Option<&str>,
    all_term: bool,
) -> Result<()> {
    let api = CourseApi::from_session()?;
    let only_current = !all_term;
    let courses = api.list_courses(only_current).await?;

    // 收集所有作业
    let pb = indicatif::ProgressBar::new(courses.len() as u64);
    pb.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("{prefix} [{bar:30}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=> "),
    );
    pb.set_prefix("扫描");

    let mut all_assignments = Vec::new();
    for course in &courses {
        pb.set_message(course.name().to_string());
        if let Ok(assignments) = api.list_assignments_for_course(course).await {
            all_assignments.extend(assignments);
        }
        pb.inc(1);
    }
    pb.finish_and_clear();

    let assignment = match id {
        Some(hash_id) => all_assignments
            .into_iter()
            .find(|a| a.hash_id == hash_id)
            .ok_or_else(|| anyhow!("未找到 ID 为 {} 的作业", hash_id))?,
        None => {
            // 交互式选择
            if all_assignments.is_empty() {
                return Err(anyhow!("暂无作业"));
            }
            all_assignments.sort_by_key(|a| a.deadline);
            display::print_assignments_list(&all_assignments, true);
            println!("请输入作业编号:");
            let idx = read_index()?.saturating_sub(1);
            all_assignments
                .into_iter()
                .nth(idx)
                .ok_or_else(|| anyhow!("无效的编号"))?
        }
    };

    if assignment.attachments.is_empty() {
        println!("{}", "该作业无附件可下载".dimmed());
        return Ok(());
    }

    let out_dir = output_dir.unwrap_or(".");
    tokio::fs::create_dir_all(out_dir).await?;

    for (i, att) in assignment.attachments.iter().enumerate() {
        println!(
            "{} [{}/{}] 下载 {} ...",
            "[*]".cyan(),
            i + 1,
            assignment.attachments.len(),
            att.name
        );
        let (filename, data) = api.download_file(&att.url).await?;
        let out_path = Path::new(out_dir).join(&filename);
        tokio::fs::write(&out_path, &data).await?;
        println!(
            "  {} {} ({})",
            "✓".green(),
            out_path.display(),
            format_size(data.len()),
        );
    }

    println!("{} 下载完成", "✓".green());
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

// ─── submit ────────────────────────────────────────────────────

pub async fn cmd_submit(
    course_id: Option<&str>,
    content_id: Option<&str>,
    file: Option<&str>,
) -> Result<()> {
    let api = CourseApi::from_session()?;

    let (actual_course, actual_content) = match (course_id, content_id) {
        (Some(cid), Some(ctid)) => {
            let resolved = resolve_course_id(&api, cid).await?;
            (resolved, ctid.to_string())
        }
        _ => {
            // 交互式选择：列出所有未完成作业
            let courses = api.list_courses(true).await?;
            let pb = indicatif::ProgressBar::new(courses.len() as u64);
            pb.set_style(
                indicatif::ProgressStyle::default_bar()
                    .template("{prefix} [{bar:30}] {pos}/{len} {msg}")
                    .unwrap()
                    .progress_chars("=> "),
            );
            pb.set_prefix("扫描");
            let mut all_assignments = Vec::new();
            for course in &courses {
                pb.set_message(course.name().to_string());
                if let Ok(assignments) = api.list_assignments_for_course(course).await {
                    all_assignments.extend(assignments);
                }
                pb.inc(1);
            }
            pb.finish_and_clear();

            all_assignments.retain(|a| a.last_attempt.is_none());
            all_assignments.sort_by_key(|a| a.deadline);

            if all_assignments.is_empty() {
                return Err(anyhow!("暂无未完成作业"));
            }

            display::print_assignments_list(&all_assignments, false);
            println!("请输入作业编号:");
            let idx = read_index()?.saturating_sub(1);
            let a = all_assignments
                .get(idx)
                .ok_or_else(|| anyhow!("无效的编号"))?;
            (a.course_id.clone(), a.content_id.clone())
        }
    };

    // 选择文件
    let file_path = match file {
        Some(f) => std::path::PathBuf::from(f),
        None => {
            // 交互式：列出当前目录文件
            let mut files: Vec<String> = Vec::new();
            let entries = std::fs::read_dir(".")?;
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    files.push(path.to_string_lossy().to_string());
                }
            }
            if files.is_empty() {
                return Err(anyhow!("当前目录无文件可提交"));
            }
            files.sort();
            println!();
            println!("{}", "── 当前目录文件 ──".bold());
            for (i, f) in files.iter().enumerate() {
                println!("  {} {}", format!("[{}]", i + 1).cyan(), f);
            }
            println!("请选择要提交的文件:");
            let idx = read_index()?.saturating_sub(1);
            let f = files.get(idx).ok_or_else(|| anyhow!("无效的编号"))?;
            std::path::PathBuf::from(f)
        }
    };

    if !file_path.exists() {
        return Err(anyhow!("文件不存在: {}", file_path.display()));
    }

    println!("{} 正在提交 {} ...", "[*]".cyan(), file_path.display());

    api.submit_assignment(&actual_course, &actual_content, &file_path)
        .await?;

    println!("{} 作业提交成功！", "✓".green());
    Ok(())
}

// ─── videos ───────────────────────────────────────────────────

pub async fn cmd_videos(course_id: Option<&str>, all_term: bool) -> Result<()> {
    let api = CourseApi::from_session()?;
    let only_current = !all_term;

    match course_id {
        Some(cid) => {
            // 指定课程
            let course = resolve_course_info(&api, cid).await?;
            let videos = api.list_videos(&course.id, course.name()).await?;
            if videos.is_empty() {
                println!("{}", "该课程暂无回放视频".dimmed());
                return Ok(());
            }
            display::print_videos(&videos);
        }
        None => {
            // 列出所有课程的回放
            let courses = api.list_courses(only_current).await?;
            let mut all_videos = Vec::new();

            let pb = indicatif::ProgressBar::new(courses.len() as u64);
            pb.set_style(
                indicatif::ProgressStyle::default_bar()
                    .template("{prefix} [{bar:30}] {pos}/{len} {msg}")
                    .unwrap()
                    .progress_chars("=> "),
            );
            pb.set_prefix("扫描课程");

            for course in &courses {
                pb.set_message(course.name().to_string());
                match api.list_videos(&course.id, course.name()).await {
                    Ok(videos) => all_videos.extend(videos),
                    Err(e) => {
                        tracing::warn!("获取 {} 的回放失败: {e:#}", course.name());
                    }
                }
                pb.inc(1);
            }
            pb.finish_and_clear();

            if all_videos.is_empty() {
                println!("{}", "暂无回放视频".dimmed());
                return Ok(());
            }

            display::print_videos(&all_videos);
        }
    }

    Ok(())
}

// ─── video-download ───────────────────────────────────────────

pub async fn cmd_video_download(
    course_id: Option<&str>,
    id: &str,
    output_dir: Option<&str>,
    all_term: bool,
) -> Result<()> {
    let api = CourseApi::from_session()?;
    let only_current = !all_term;

    // 查找视频：可以是 hash_id 或 index
    let video = if let Some(cid) = course_id {
        let course = resolve_course_info(&api, cid).await?;
        let videos = api.list_videos(&course.id, course.name()).await?;
        find_video(videos, id)?
    } else {
        // 在所有课程中搜索
        let courses = api.list_courses(only_current).await?;
        let mut all_videos = Vec::new();
        for course in &courses {
            if let Ok(videos) = api.list_videos(&course.id, course.name()).await {
                all_videos.extend(videos);
            }
        }
        find_video(all_videos, id)?
    };

    println!(
        "{} 获取视频信息: {} - {} ({})",
        "[*]".cyan(),
        video.course_name,
        video.title,
        video.time,
    );

    let detail = api.get_video_detail(&video).await?;
    let total_segments = detail.playlist.segments.len();
    println!("{} 共 {} 个片段，开始下载...", "[*]".cyan(), total_segments,);

    let out_dir = output_dir.unwrap_or(".");
    tokio::fs::create_dir_all(out_dir).await?;

    // 使用 hash_id 作为临时目录名（支持断点续传）
    let tmp_dir = Path::new(out_dir).join(format!(".video_tmp_{}", video.hash_id));
    tokio::fs::create_dir_all(&tmp_dir).await?;

    let pb = indicatif::ProgressBar::new(total_segments as u64);
    pb.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("{prefix} [{bar:40}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=> "),
    );
    pb.set_prefix("下载");

    let mut current_key: Option<&m3u8_rs::Key> = None;
    let mut aes_key_cache: Option<([u8; 16], String)> = None;
    let mut segment_paths = Vec::new();

    for i in 0..total_segments {
        let seg = &detail.playlist.segments[i];

        // 更新加密密钥
        if let Some(new_key) = &seg.key {
            current_key = Some(new_key);
        }

        let seg_url = detail
            .base_url
            .join(&seg.uri)
            .context("拼接片段 URL 失败")?;

        // 使用 segment URI 命名（与 pku3b 一致，支持断点续传）
        let seg_filename = seg.uri.replace(['/', '\\'], "_");
        let seg_path = tmp_dir.join(seg_filename).with_extension("ts");

        if !seg_path.exists() {
            let mut data = api.download_segment(seg_url.as_str()).await?;

            // AES-128 解密
            if let Some(key) = current_key {
                if matches!(key.method, m3u8_rs::KeyMethod::AES128) {
                    let uri = key.uri.as_ref().context("AES 密钥 URI 缺失")?;

                    let aes_key = if let Some((cached_key, cached_uri)) = &aes_key_cache {
                        if cached_uri == uri {
                            *cached_key
                        } else {
                            let k = api.get_aes_key(uri).await?;
                            aes_key_cache = Some((k, uri.clone()));
                            k
                        }
                    } else {
                        let k = api.get_aes_key(uri).await?;
                        aes_key_cache = Some((k, uri.clone()));
                        k
                    };

                    let iv = api::build_iv(key, detail.playlist.media_sequence, i);
                    data = api::decrypt_segment(&aes_key, &iv, &data)?.into();
                }
            }

            // 原子写入：先写临时文件再重命名
            let tmp_path = seg_path.with_extension("tmp");
            tokio::fs::write(&tmp_path, &data).await?;
            tokio::fs::rename(&tmp_path, &seg_path).await?;
        }

        segment_paths.push(seg_path);
        pb.inc(1);
    }
    pb.finish_and_clear();

    // 合并所有 ts 片段
    println!("{} 合并视频片段...", "[*]".cyan());
    let merged_path = tmp_dir.join("merged.ts");
    {
        use tokio::io::AsyncWriteExt;
        let mut merged_file = tokio::fs::File::create(&merged_path).await?;
        for seg_path in &segment_paths {
            let data = tokio::fs::read(seg_path).await?;
            merged_file.write_all(&data).await?;
        }
        merged_file.flush().await?;
    }

    // 用 ffmpeg 转换为 mp4
    let safe_title = video.title.replace(['/', '\\'], "_");
    let safe_time = video.time.replace(['/', '\\', ':'], "-");
    let output_file = Path::new(out_dir).join(format!(
        "{}_{}_{}.mp4",
        video.course_name.replace(['/', '\\'], "_"),
        safe_title,
        safe_time,
    ));

    println!("{} 转换为 MP4 (需要 ffmpeg)...", "[*]".cyan());
    let ffmpeg_status = tokio::process::Command::new("ffmpeg")
        .args(["-y", "-hide_banner", "-loglevel", "quiet"])
        .arg("-i")
        .arg(&merged_path)
        .args(["-c", "copy"])
        .arg(&output_file)
        .status()
        .await
        .context("执行 ffmpeg 失败，请确保已安装 ffmpeg")?;

    if !ffmpeg_status.success() {
        return Err(anyhow!(
            "ffmpeg 转换失败 (exit code: {:?})",
            ffmpeg_status.code()
        ));
    }

    // 清理临时文件
    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;

    println!("{} 下载完成: {}", "✓".green(), output_file.display(),);
    Ok(())
}

// ─── announcements ────────────────────────────────────────────

pub async fn cmd_announcements(course_id: Option<&str>) -> Result<()> {
    let api = CourseApi::from_session()?;

    match course_id {
        Some(cid) => {
            let course = resolve_course_info(&api, cid).await?;
            let announcements = api.list_announcements(&course.id).await?;
            if announcements.is_empty() {
                println!("{} {} 暂无公告", "○".dimmed(), course.name());
                return Ok(());
            }
            println!();
            println!(
                "{} {} (共 {} 条)",
                "──".bold(),
                course.name().bold(),
                announcements.len()
            );
            for ann in &announcements {
                display::print_announcement(ann);
            }
        }
        None => {
            let courses = api.list_courses(true).await?;
            if courses.is_empty() {
                println!("{}", "暂无课程".dimmed());
                return Ok(());
            }

            let pb = indicatif::ProgressBar::new(courses.len() as u64);
            pb.set_style(
                indicatif::ProgressStyle::default_bar()
                    .template("{prefix} [{bar:30}] {pos}/{len} {msg}")
                    .unwrap()
                    .progress_chars("=> "),
            );
            pb.set_prefix("扫描公告");

            let mut all_announcements = Vec::new();
            for course in &courses {
                pb.set_message(course.name().to_string());
                match api.list_announcements_for_course(course).await {
                    Ok(anns) => all_announcements.extend(anns),
                    Err(e) => {
                        tracing::warn!("获取 {} 的公告失败: {e:#}", course.name());
                    }
                }
                pb.inc(1);
            }
            pb.finish_and_clear();

            if all_announcements.is_empty() {
                println!("{}", "当前学期暂无公告".dimmed());
                return Ok(());
            }

            println!();
            println!("{} 共 {} 条公告", "──".bold(), all_announcements.len());
            for summary in &all_announcements {
                display::print_announcement_summary(summary);
            }
        }
    }

    println!();
    Ok(())
}

// ─── browse ────────────────────────────────────────────────────

pub async fn cmd_browse(course_id: Option<&str>) -> Result<()> {
    let api = CourseApi::from_session()?;

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

    let entries = api.list_course_entries(&course).await?;
    if entries.is_empty() {
        println!("{}", "课程无内容".dimmed());
        return Ok(());
    }

    display::print_course_entries(&course, &entries);

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

/// 解析课程 ID（数字索引 / 内部 ID / 课程名搜索）
async fn resolve_course_id(api: &CourseApi, input: &str) -> Result<String> {
    Ok(resolve_course_info(api, input).await?.id)
}

/// 解析课程 ID 并返回完整课程信息
async fn resolve_course_info(api: &CourseApi, input: &str) -> Result<api::CourseInfo> {
    if input.starts_with('_') && input.ends_with("_1") {
        let courses = api.list_courses(false).await?;
        if let Some(c) = courses.into_iter().find(|c| c.id == input) {
            return Ok(c);
        }
        // 也尝试在所有课程中搜索
        let courses = api.list_courses(false).await?;
        return courses
            .into_iter()
            .find(|c| c.id == input)
            .ok_or_else(|| anyhow!("未找到 ID 为 {} 的课程", input));
    }

    let courses = api.list_courses(false).await?;
    let total = courses.len();

    if let Ok(idx) = input.parse::<usize>() {
        return courses
            .into_iter()
            .nth(idx.saturating_sub(1))
            .ok_or_else(|| anyhow!("无效的课程编号 {idx}，共 {total} 门课程"));
    }

    courses
        .into_iter()
        .find(|c| c.name().contains(input) || c.long_title.contains(input))
        .ok_or_else(|| anyhow!("未找到匹配的课程 \"{input}\"。使用 `course courses` 查看课程列表"))
}

/// 通过 hash_id 或序号查找视频
fn find_video(videos: Vec<api::VideoInfo>, id: &str) -> Result<api::VideoInfo> {
    // 先尝试作为 hash_id
    if let Some(v) = videos.iter().find(|v| v.hash_id == id) {
        return Ok(v.clone());
    }

    // 再尝试作为数字序号
    if let Ok(idx) = id.parse::<usize>() {
        return videos
            .into_iter()
            .nth(idx.saturating_sub(1))
            .ok_or_else(|| anyhow!("无效的视频编号 {idx}"));
    }

    Err(anyhow!(
        "未找到 ID 为 {} 的视频。使用 `course videos` 查看列表",
        id
    ))
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
    line.parse::<usize>().map_err(|_| anyhow!("请输入有效数字"))
}
