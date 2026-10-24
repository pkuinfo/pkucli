//! 所有子命令的实现

use crate::api::{self, CourseApi};
use crate::display;
use crate::ta::{
    self, fmt_score, text_to_html, Attempt, Column, Content, GradeInput, NewAssignment, Student,
    TaApi,
};
use crate::TaAction;
use anyhow::{anyhow, bail, Context, Result};
use colored::Colorize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

// ─── ta（助教）──────────────────────────────────────────────
//
// 写操作一律先打印预览，加 `--yes` 才真正写入。

pub async fn cmd_ta(action: TaAction) -> Result<()> {
    match action {
        TaAction::Ls { course, folder } => cmd_ta_ls(&course, folder.as_deref()).await,
        TaAction::Columns { course } => cmd_ta_columns(&course).await,
        TaAction::Roster { course } => cmd_ta_roster(&course).await,
        TaAction::Attempts {
            course,
            column,
            needs_grading,
        } => cmd_ta_attempts(&course, &column, needs_grading).await,
        TaAction::Download {
            course,
            column,
            output,
            needs_grading,
        } => cmd_ta_download(&course, &column, output.as_deref(), needs_grading).await,
        TaAction::Post {
            course,
            folder,
            file,
            title,
            desc,
            hidden,
            yes,
        } => {
            cmd_ta_post(
                &course,
                &folder,
                &file,
                title.as_deref(),
                desc.as_deref(),
                hidden,
                yes,
            )
            .await
        }
        TaAction::Assign {
            course,
            folder,
            title,
            due,
            points,
            attempts,
            file,
            instructions,
            hidden,
            yes,
        } => {
            cmd_ta_assign(AssignArgs {
                course: &course,
                folder: &folder,
                title: &title,
                due: &due,
                points,
                attempts,
                files: &file,
                instructions: instructions.as_deref(),
                hidden,
                yes,
            })
            .await
        }
        TaAction::Grade {
            course,
            column,
            student,
            score,
            feedback,
            file,
            yes,
        } => {
            cmd_ta_grade(GradeArgs {
                course: &course,
                column: &column,
                student: &student,
                score: &score,
                feedback: feedback.as_deref(),
                files: &file,
                yes,
            })
            .await
        }
        TaAction::GradeBatch {
            course,
            column,
            csv,
            files_dir,
            yes,
        } => cmd_ta_grade_batch(&course, &column, &csv, files_dir.as_deref(), yes).await,
        TaAction::Rm {
            course,
            content_id,
            yes,
        } => cmd_ta_rm(&course, &content_id, yes).await,
        TaAction::Videos { course } => cmd_ta_videos(&course).await,
        TaAction::Publish {
            course,
            videos,
            yes,
        } => cmd_ta_set_published(&course, &videos, true, yes).await,
        TaAction::Unpublish {
            course,
            videos,
            yes,
        } => cmd_ta_set_published(&course, &videos, false, yes).await,
    }
}

const TEMPLATE_NAME: &str = "成绩模板.csv";

async fn open_ta(course: &str) -> Result<TaApi> {
    let api = CourseApi::from_session()?;
    let info = resolve_course_info(&api, course).await?;
    println!("{} {}", "课程:".dimmed(), info.title());
    Ok(TaApi::new(api, info.id))
}

fn preview_only() {
    println!(
        "\n{} 以上只是预览，没有写入。确认无误后加 {} 重新执行。",
        "!".yellow().bold(),
        "--yes".bold()
    );
}

fn is_bb_id(s: &str) -> bool {
    s.starts_with('_') && s.ends_with("_1")
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

async fn resolve_folder(ta: &TaApi, input: &str) -> Result<Content> {
    let folder = if is_bb_id(input) {
        ta.content(input).await?
    } else {
        let folders = ta.top_folders().await?;
        folders
            .iter()
            .find(|c| c.title == input)
            .or_else(|| folders.iter().find(|c| c.title.contains(input)))
            .cloned()
            .ok_or_else(|| anyhow!("未找到文件夹 \"{input}\"。用 `course ta ls <课程>` 查看"))?
    };
    if folder.handler() != ta::FOLDER {
        bail!(
            "「{}」不是文件夹（类型: {}）",
            folder.title,
            display::kind_label(folder.handler())
        );
    }
    Ok(folder)
}

/// 定位作业成绩列；计算列一律拒绝
async fn resolve_column(ta: &TaApi, input: &str) -> Result<Column> {
    let col = if is_bb_id(input) {
        ta.column(input).await?
    } else {
        let cols = ta.columns().await?;
        cols.iter()
            .find(|c| c.name == input)
            .or_else(|| cols.iter().find(|c| c.name.contains(input)))
            .cloned()
            .ok_or_else(|| {
                anyhow!("未找到成绩列 \"{input}\"。用 `course ta columns <课程>` 查看")
            })?
    };
    if col.is_calculated() {
        bail!(
            "「{}」是计算列（总计/加权总计之类），不允许写入或按作业处理",
            col.name
        );
    }
    if col.kind() != "Attempts" {
        bail!(
            "「{}」不是作业列（类型: {}），没有学生提交",
            col.name,
            col.kind()
        );
    }
    Ok(col)
}

fn resolve_student<'a>(students: &'a [Student], input: &str) -> Result<&'a Student> {
    if let Some(s) = students.iter().find(|s| s.user_name == input) {
        return Ok(s);
    }
    let by_name: Vec<_> = students.iter().filter(|s| s.name == input).collect();
    match by_name.as_slice() {
        [one] => Ok(one),
        [] => bail!("名单里没有学号或姓名为「{input}」的学生"),
        _ => bail!("有 {} 名学生叫「{input}」，请改用学号", by_name.len()),
    }
}

/// 每个学生的全部提交，按时间排好；只保留名单里的学生（预览账号自然被排除）
fn group_attempts(students: &[Student], attempts: Vec<Attempt>) -> HashMap<String, Vec<Attempt>> {
    let ids = ta::index_by_user_id(students);
    let mut map: HashMap<String, Vec<Attempt>> = HashMap::new();
    for a in attempts {
        if ids.contains_key(a.user_id.as_str()) {
            map.entry(a.user_id.clone()).or_default().push(a);
        }
    }
    for list in map.values_mut() {
        list.sort_by(|a, b| a.created.cmp(&b.created));
    }
    map
}

fn file_size(path: &Path) -> Result<usize> {
    let meta =
        std::fs::metadata(path).with_context(|| format!("找不到文件: {}", path.display()))?;
    if !meta.is_file() {
        bail!("不是文件: {}", path.display());
    }
    Ok(meta.len() as usize)
}

// ─── 只读 ──────────────────────────────────────────────────────

async fn cmd_ta_ls(course: &str, folder: Option<&str>) -> Result<()> {
    let ta = open_ta(course).await?;
    match folder {
        None => {
            let items = ta.top_folders().await?;
            display::print_ta_contents("顶层内容区", "", &items);
        }
        Some(f) => {
            let c = resolve_folder(&ta, f).await?;
            let items = ta.children(&c.id).await?;
            display::print_ta_contents(&c.title, &c.id, &items);
        }
    }
    Ok(())
}

async fn cmd_ta_columns(course: &str) -> Result<()> {
    let ta = open_ta(course).await?;
    display::print_ta_columns(&ta.columns().await?);
    Ok(())
}

async fn cmd_ta_roster(course: &str) -> Result<()> {
    let ta = open_ta(course).await?;
    display::print_ta_roster(&ta.students().await?);
    Ok(())
}

async fn cmd_ta_attempts(course: &str, column: &str, needs_grading: bool) -> Result<()> {
    let ta = open_ta(course).await?;
    let col = resolve_column(&ta, column).await?;
    let students = ta.students().await?;
    let by_user = group_attempts(&students, ta.attempts(&col.id).await?);
    display::print_ta_attempts(&col, &students, &by_user, needs_grading);
    Ok(())
}

async fn cmd_ta_download(
    course: &str,
    column: &str,
    output: Option<&Path>,
    needs_grading: bool,
) -> Result<()> {
    let ta = open_ta(course).await?;
    let col = resolve_column(&ta, column).await?;
    let students = ta.students().await?;
    let by_user = group_attempts(&students, ta.attempts(&col.id).await?);
    let out = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(sanitize(&col.name)));
    std::fs::create_dir_all(&out).with_context(|| format!("创建目录失败: {}", out.display()))?;

    let (mut saved, mut skipped) = (0, 0);
    let mut template = vec![vec![
        "学号".to_string(),
        "姓名".into(),
        "分数".into(),
        "评语".into(),
    ]];
    for s in &students {
        let Some(list) = by_user.get(&s.user_id) else {
            continue;
        };
        let latest = list.last().expect("non-empty attempt list");
        if needs_grading && latest.status.as_deref() != Some("NeedsGrading") {
            continue;
        }
        template.push(vec![
            s.user_name.clone(),
            s.name.clone(),
            latest.score.map(fmt_score).unwrap_or_default(),
            String::new(),
        ]);
        let base = out.join(sanitize(&format!("{}_{}", s.user_name, s.name)));
        for (i, a) in list.iter().enumerate() {
            let dir = if list.len() > 1 {
                base.join(format!("第{}次", i + 1))
            } else {
                base.clone()
            };
            for f in ta.attempt_files(&a.id).await? {
                let dest = dir.join(sanitize(&f.name));
                if dest.exists() {
                    skipped += 1;
                    continue;
                }
                let bytes = ta.download_attempt_file(&a.id, &f).await?;
                std::fs::create_dir_all(&dir)?;
                std::fs::write(&dest, &bytes)
                    .with_context(|| format!("写入失败: {}", dest.display()))?;
                println!("  {} {}", "↓".green(), dest.display());
                saved += 1;
            }
        }
    }

    let tpl = out.join(TEMPLATE_NAME);
    if tpl.exists() {
        println!("{} {} 已存在，未覆盖", "·".dimmed(), tpl.display());
    } else {
        std::fs::write(&tpl, to_csv(&template)).context("写入成绩模板失败")?;
        println!(
            "{} 成绩模板: {}（填好分数和评语后用 `course ta grade-batch` 导入）",
            "✓".green(),
            tpl.display()
        );
    }
    println!(
        "{} 下载 {} 个文件，跳过已存在的 {} 个，共 {} 名学生 → {}",
        "✓".green(),
        saved,
        skipped,
        template.len() - 1,
        out.display()
    );
    Ok(())
}

// ─── 写：内容 ──────────────────────────────────────────────────

async fn cmd_ta_post(
    course: &str,
    folder: &str,
    file: &Path,
    title: Option<&str>,
    desc: Option<&str>,
    hidden: bool,
    yes: bool,
) -> Result<()> {
    let size = file_size(file)?;
    let ta = open_ta(course).await?;
    let parent = resolve_folder(&ta, folder).await?;
    let title = title.map(String::from).unwrap_or_else(|| {
        file.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    });

    println!("{}", "将新建文件条目:".bold());
    println!("  文件夹  {} {}", parent.title, parent.id.dimmed());
    println!("  标题    {title}");
    println!("  文件    {} ({})", file.display(), format_size(size));
    if let Some(d) = desc {
        println!("  说明    {d}");
    }
    println!(
        "  可见性  {}",
        if hidden {
            "对学生隐藏".yellow()
        } else {
            "学生立即可见".green()
        }
    );
    if !yes {
        preview_only();
        return Ok(());
    }

    let item = ta
        .create_file_item(&parent.id, &title, desc, file, !hidden)
        .await?;
    println!("{} 已创建 {} {}", "✓".green(), item.title, item.id.dimmed());
    Ok(())
}

pub struct AssignArgs<'a> {
    pub course: &'a str,
    pub folder: &'a str,
    pub title: &'a str,
    pub due: &'a str,
    pub points: f64,
    pub attempts: u32,
    pub files: &'a [PathBuf],
    pub instructions: Option<&'a str>,
    pub hidden: bool,
    pub yes: bool,
}

fn parse_due(input: &str) -> Result<chrono::DateTime<chrono::Local>> {
    use chrono::{NaiveDate, NaiveDateTime, TimeZone};
    let naive = NaiveDateTime::parse_from_str(input, "%Y-%m-%d %H:%M")
        .or_else(|_| {
            NaiveDate::parse_from_str(input, "%Y-%m-%d")
                .map(|d| d.and_hms_opt(23, 59, 0).expect("valid time"))
        })
        .map_err(|_| anyhow!("截止时间格式应为 \"2026-11-01 23:59\" 或 \"2026-11-01\""))?;
    chrono::Local
        .from_local_datetime(&naive)
        .single()
        .ok_or_else(|| anyhow!("截止时间 {input} 无法换算成本地时间"))
}

async fn cmd_ta_assign(a: AssignArgs<'_>) -> Result<()> {
    let due = parse_due(a.due)?;
    if a.points <= 0.0 {
        bail!("满分必须大于 0");
    }
    let mut sizes = Vec::new();
    for f in a.files {
        sizes.push(file_size(f)?);
    }
    let ta = open_ta(a.course).await?;
    let parent = resolve_folder(&ta, a.folder).await?;

    println!("{}", "将新建作业:".bold());
    println!("  文件夹    {} {}", parent.title, parent.id.dimmed());
    println!("  标题      {}", a.title);
    println!("  截止      {}", due.format("%Y-%m-%d %H:%M"));
    println!("  满分      {}", fmt_score(a.points));
    println!(
        "  提交次数  {}",
        if a.attempts == 0 {
            "不限".to_string()
        } else {
            a.attempts.to_string()
        }
    );
    for (f, s) in a.files.iter().zip(&sizes) {
        println!("  附件      {} ({})", f.display(), format_size(*s));
    }
    if let Some(i) = a.instructions {
        println!("  说明      {i}");
    }
    println!(
        "  可见性    {}",
        if a.hidden {
            "对学生隐藏".yellow()
        } else {
            "学生立即可见".green()
        }
    );
    if due < chrono::Local::now() {
        println!("  {} 截止时间已经过去", "!".yellow().bold());
    }
    if !a.yes {
        preview_only();
        return Ok(());
    }

    let due_utc = due
        .with_timezone(&chrono::Utc)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let created = ta
        .create_assignment(&NewAssignment {
            parent_id: &parent.id,
            title: a.title,
            instructions: a.instructions,
            files: a.files,
            due_utc: &due_utc,
            points: a.points,
            attempts: a.attempts,
            visible: !a.hidden,
        })
        .await?;
    println!(
        "{} 已创建作业 {}  成绩列 {}",
        "✓".green(),
        created.content_id,
        created.grade_column_id
    );
    Ok(())
}

async fn cmd_ta_rm(course: &str, content_id: &str, yes: bool) -> Result<()> {
    let ta = open_ta(course).await?;
    let item = ta.content(content_id).await?;
    println!("{}", "将删除:".bold());
    println!(
        "  {} [{}] {}",
        item.title,
        display::kind_label(item.handler()),
        item.id.dimmed()
    );

    if item.handler() == ta::FOLDER && !ta.children(&item.id).await?.is_empty() {
        bail!("文件夹不是空的，拒绝删除。请先逐个删除里面的条目");
    }
    if item.handler() == ta::ASSIGNMENT {
        let col = ta
            .columns()
            .await?
            .into_iter()
            .find(|c| c.content_id.as_deref() == Some(item.id.as_str()));
        if let Some(col) = col {
            let students = ta.students().await?;
            let n = group_attempts(&students, ta.attempts(&col.id).await?).len();
            if n > 0 {
                bail!("已有 {n} 名学生提交，拒绝删除（会连带删掉他们的提交和成绩）");
            }
            println!("  连同成绩列 {} {}", col.name, col.id.dimmed());
        }
    }
    if !yes {
        preview_only();
        return Ok(());
    }
    ta.delete_content(&item.id).await?;
    println!("{} 已删除", "✓".green());
    Ok(())
}

// ─── 课堂实录 ──────────────────────────────────────────────────

async fn cmd_ta_videos(course: &str) -> Result<()> {
    let ta = open_ta(course).await?;
    display::print_ta_recordings(&ta.recordings().await?);
    Ok(())
}

/// 按序号（1 起，按时间排序）、录像 ID 或标题片段找录像
fn resolve_recordings<'a>(
    list: &'a [ta::Recording],
    inputs: &[String],
) -> Result<Vec<&'a ta::Recording>> {
    let mut out: Vec<&ta::Recording> = Vec::new();
    for input in inputs {
        let found = if let Some(r) = list.iter().find(|r| &r.sub_id == input) {
            r
        } else if let Some(idx) = input
            .parse::<usize>()
            .ok()
            .filter(|_| !input.starts_with('0'))
        {
            // 「09」这类以 0 开头的当作日期片段，不当序号
            list.get(idx.wrapping_sub(1))
                .ok_or_else(|| anyhow!("无效的录像序号 {idx}，共 {} 节", list.len()))?
        } else {
            let hits: Vec<_> = list
                .iter()
                .filter(|r| r.title.contains(input.as_str()))
                .collect();
            match hits.as_slice() {
                [one] => *one,
                [] => bail!("没有标题包含「{input}」的录像。用 `course ta videos <课程>` 查看"),
                _ => bail!(
                    "有 {} 节录像标题包含「{input}」，请写得更具体或改用序号",
                    hits.len()
                ),
            }
        };
        if !out.iter().any(|r| r.sub_id == found.sub_id) {
            out.push(found);
        }
    }
    Ok(out)
}

async fn cmd_ta_set_published(
    course: &str,
    videos: &[String],
    publish: bool,
    yes: bool,
) -> Result<()> {
    let ta = open_ta(course).await?;
    let list = ta.recordings().await?;
    let targets: Vec<&ta::Recording> = if videos.is_empty() {
        list.iter().filter(|r| !r.published).collect()
    } else {
        resolve_recordings(&list, videos)?
    };
    let (verb, todo): (&str, Vec<_>) = (
        if publish { "发布" } else { "撤销" },
        targets
            .into_iter()
            .filter(|r| r.published != publish)
            .collect(),
    );
    if todo.is_empty() {
        println!("{}", format!("没有需要{verb}的录像").dimmed());
        return Ok(());
    }
    println!("{}", format!("将{verb}:").bold());
    for r in &todo {
        println!("  {}  {}  {}", r.time, r.title, r.sub_id.dimmed());
    }
    if !publish {
        println!("  {} 撤销后学生将看不到这些录像", "!".yellow().bold());
    }
    if !yes {
        preview_only();
        return Ok(());
    }
    for r in &todo {
        ta.set_published(r, publish).await?;
        println!("  {} 已{verb} {}", "✓".green(), r.title);
    }
    Ok(())
}

// ─── 写：登分 ──────────────────────────────────────────────────

fn parse_score(input: &str, col: &Column) -> Result<f64> {
    let score: f64 = input
        .trim()
        .parse()
        .map_err(|_| anyhow!("分数「{input}」不是数字"))?;
    if score < 0.0 || !score.is_finite() {
        bail!("分数不能为负: {input}");
    }
    if let Some(max) = col.possible() {
        if score > max {
            bail!("分数 {input} 超过满分 {}", fmt_score(max));
        }
    }
    Ok(score)
}

pub struct GradeArgs<'a> {
    pub course: &'a str,
    pub column: &'a str,
    pub student: &'a str,
    pub score: &'a str,
    pub feedback: Option<&'a str>,
    pub files: &'a [PathBuf],
    pub yes: bool,
}

async fn cmd_ta_grade(a: GradeArgs<'_>) -> Result<()> {
    for f in a.files {
        file_size(f)?;
    }
    let ta = open_ta(a.course).await?;
    let col = resolve_column(&ta, a.column).await?;
    let score = parse_score(a.score, &col)?;
    // 名单默认排除预览账号；只有显式写出 xxx_previewuser 时才纳入（用于测试评分流程）
    let students = if a.student.ends_with(ta::PREVIEW_SUFFIX) {
        ta.students_with_preview().await?
    } else {
        ta.students().await?
    };
    let s = resolve_student(&students, a.student)?;
    let by_user = group_attempts(&students, ta.attempts(&col.id).await?);
    let attempt = by_user
        .get(&s.user_id)
        .and_then(|l| l.last())
        .ok_or_else(|| {
            anyhow!(
                "{} {} 在「{}」下没有提交，目前只支持给已提交的作业登分",
                s.user_name,
                s.name,
                col.name
            )
        })?;

    println!("{}", "将登分:".bold());
    println!("  作业  {}", col.name);
    println!("  学生  {} {}", s.user_name, s.name);
    println!(
        "  分数  {} → {}",
        attempt
            .score
            .map(fmt_score)
            .unwrap_or_else(|| "未评".into()),
        fmt_score(score).bold()
    );
    if let Some(fb) = a.feedback {
        println!("  评语  {}", fb.replace('\n', "\n        "));
    }
    for f in a.files {
        println!("  反馈文件  {}", f.display());
    }
    if !a.yes {
        preview_only();
        return Ok(());
    }

    let after = ta
        .grade_attempt(
            &col.id,
            &attempt.id,
            &GradeInput {
                score,
                feedback_html: a.feedback.map(text_to_html),
                files: a.files,
            },
        )
        .await?;
    println!(
        "{} 已登分 {}，状态 {}",
        "✓".green(),
        after.score.map(fmt_score).unwrap_or_default(),
        display::status_label(after.status.as_deref())
    );
    Ok(())
}

struct BatchRow<'a> {
    student: &'a Student,
    attempt: Attempt,
    score: f64,
    feedback: Option<String>,
    files: Vec<PathBuf>,
}

/// 反馈文件：files_dir 里文件名以学号开头的文件（不看子目录，免得把学生原提交传回去）
fn feedback_files(dir: &Path, user_name: &str) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).with_context(|| format!("读取目录失败: {}", dir.display()))?
    {
        let e = e?;
        if e.file_type()?.is_file() && e.file_name().to_string_lossy().starts_with(user_name) {
            out.push(e.path());
        }
    }
    out.sort();
    Ok(out)
}

async fn cmd_ta_grade_batch(
    course: &str,
    column: &str,
    csv: &Path,
    files_dir: Option<&Path>,
    yes: bool,
) -> Result<()> {
    let text =
        std::fs::read_to_string(csv).with_context(|| format!("读取 {} 失败", csv.display()))?;
    let ta = open_ta(course).await?;
    let col = resolve_column(&ta, column).await?;
    let students = ta.students().await?;
    let mut by_user = group_attempts(&students, ta.attempts(&col.id).await?);

    let mut rows = Vec::new();
    let mut errors = Vec::new();
    let mut blank = 0;
    let mut seen = std::collections::HashSet::new();
    for (lineno, cells) in parse_csv(text.trim_start_matches('\u{feff}'))
        .into_iter()
        .enumerate()
    {
        let line = lineno + 1;
        let id = cells.first().map(|s| s.trim()).unwrap_or_default();
        if id.is_empty() || (line == 1 && id == "学号") {
            continue;
        }
        // 先查名单再看分数：学号写错的行即使分数留空也要报出来
        let Some(student) = students.iter().find(|s| s.user_name == id) else {
            errors.push(format!("第 {line} 行: 名单里没有学号 {id}"));
            continue;
        };
        let raw_score = cells.get(2).map(|s| s.trim()).unwrap_or_default();
        if raw_score.is_empty() {
            blank += 1;
            continue;
        }
        if !seen.insert(id.to_string()) {
            errors.push(format!("第 {line} 行: 学号 {id} 重复出现"));
            continue;
        }
        let score = match parse_score(raw_score, &col) {
            Ok(s) => s,
            Err(e) => {
                errors.push(format!("第 {line} 行: {e}"));
                continue;
            }
        };
        let Some(attempt) = by_user.get_mut(&student.user_id).and_then(|l| l.pop()) else {
            errors.push(format!("第 {line} 行: {id} {} 没有提交", student.name));
            continue;
        };
        let feedback = cells
            .get(3)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let files = match files_dir {
            Some(d) => feedback_files(d, id)?,
            None => Vec::new(),
        };
        rows.push(BatchRow {
            student,
            attempt,
            score,
            feedback,
            files,
        });
    }
    if !errors.is_empty() {
        for e in &errors {
            println!("  {} {e}", "✗".red());
        }
        bail!("CSV 有 {} 处问题，全部未写入", errors.len());
    }

    println!("{} {}", "将批量登分:".bold(), col.name);
    for r in &rows {
        let old = r
            .attempt
            .score
            .map(fmt_score)
            .unwrap_or_else(|| "未评".into());
        let mut extra = Vec::new();
        if let Some(fb) = &r.feedback {
            let short: String = fb.chars().take(20).collect();
            let more = if fb.chars().count() > 20 { "…" } else { "" };
            extra.push(format!("评语「{short}{more}」"));
        }
        if !r.files.is_empty() {
            extra.push(format!("{} 个反馈文件", r.files.len()));
        }
        println!(
            "  {}  {:<6} {} → {}  {}",
            r.student.user_name,
            r.student.name,
            old,
            fmt_score(r.score).bold(),
            extra.join("、").dimmed()
        );
    }
    println!(
        "\n共 {} 人；CSV 里分数留空、跳过的 {} 人",
        rows.len().to_string().bold(),
        blank
    );
    if !yes {
        preview_only();
        return Ok(());
    }

    let mut failed = 0;
    for r in &rows {
        let res = ta
            .grade_attempt(
                &col.id,
                &r.attempt.id,
                &GradeInput {
                    score: r.score,
                    feedback_html: r.feedback.as_deref().map(text_to_html),
                    files: &r.files,
                },
            )
            .await;
        match res {
            Ok(_) => println!(
                "  {} {} {}",
                "✓".green(),
                r.student.user_name,
                r.student.name
            ),
            Err(e) => {
                failed += 1;
                println!(
                    "  {} {} {}: {e:#}",
                    "✗".red(),
                    r.student.user_name,
                    r.student.name
                );
            }
        }
    }
    if failed > 0 {
        bail!("{failed} 人登分失败，其余 {} 人已写入", rows.len() - failed);
    }
    println!("{} 全部 {} 人已写入并核对", "✓".green(), rows.len());
    Ok(())
}

// ─── CSV ───────────────────────────────────────────────────────

/// 简单 CSV 解析：支持双引号包裹和 "" 转义，不支持单元格内换行
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    text.lines()
        .map(|line| {
            let mut cells = Vec::new();
            let mut cur = String::new();
            let mut quoted = false;
            let mut chars = line.trim_end_matches('\r').chars().peekable();
            while let Some(c) = chars.next() {
                match (c, quoted) {
                    ('"', true) if chars.peek() == Some(&'"') => {
                        cur.push('"');
                        chars.next();
                    }
                    ('"', _) => quoted = !quoted,
                    (',', false) => cells.push(std::mem::take(&mut cur)),
                    _ => cur.push(c),
                }
            }
            cells.push(cur);
            cells
        })
        .collect()
}

/// 带 BOM 的 CSV，Excel 打开不乱码
fn to_csv(rows: &[Vec<String>]) -> String {
    let mut out = String::from('\u{feff}');
    for row in rows {
        let line: Vec<String> = row
            .iter()
            .map(|c| {
                if c.contains([',', '"', '\n']) {
                    format!("\"{}\"", c.replace('"', "\"\""))
                } else {
                    c.clone()
                }
            })
            .collect();
        out.push_str(&line.join(","));
        out.push_str("\r\n");
    }
    out
}

// ─── helpers ───────────────────────────────────────────────────

/// 解析课程 ID（数字索引 / 内部 ID / 课程名搜索）
async fn resolve_course_id(api: &CourseApi, input: &str) -> Result<String> {
    Ok(resolve_course_info(api, input).await?.id)
}

/// 解析课程 ID 并返回完整课程信息
pub(crate) async fn resolve_course_info(api: &CourseApi, input: &str) -> Result<api::CourseInfo> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_roundtrip_with_commas_and_quotes() {
        let rows = vec![vec![
            "2100011234".to_string(),
            "张三".into(),
            "95".into(),
            "很好，\"注意\"符号".into(),
        ]];
        let text = to_csv(&rows);
        let parsed = parse_csv(text.trim_start_matches('\u{feff}'));
        assert_eq!(parsed[0], rows[0]);
    }

    #[test]
    fn due_date_only_defaults_to_2359() {
        let d = parse_due("2026-11-01").unwrap();
        assert_eq!(d.format("%H:%M").to_string(), "23:59");
        assert!(parse_due("11/01").is_err());
    }
}
