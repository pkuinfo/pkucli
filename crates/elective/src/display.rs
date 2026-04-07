//! 终端格式化输出

use crate::api::{CourseData, SupplementCourse};
use crate::config::AutoElectCourse;
use colored::Colorize;

/// 打印选课结果
pub fn print_results(courses: &[CourseData]) {
    println!("{}", "── 选课结果 ──".bold());
    println!();

    if courses.is_empty() {
        println!("  {}", "暂无选课记录".dimmed());
        return;
    }

    for (i, c) in courses.iter().enumerate() {
        let status_colored = if c.status.contains("已选上") {
            c.status.green().to_string()
        } else {
            c.status.red().to_string()
        };

        println!(
            "  {} {} {} {}",
            format!("[{}]", i + 1).cyan(),
            c.name.bold(),
            format!("({} 班号:{} {})", c.teacher, c.class_id, c.category).dimmed(),
            status_colored,
        );
        if !c.classroom.is_empty() {
            println!(
                "      {} {} | {} | {} {}学时",
                c.department.dimmed(),
                c.classroom,
                c.credit,
                c.hours,
                "周".dimmed(),
            );
        }
    }
    println!();
    println!(
        "{}",
        format!("共 {} 门课程", courses.len()).dimmed()
    );
}

/// 打印补退选课程列表
pub fn print_supplements(courses: &[SupplementCourse], page: usize, total_pages: usize) {
    println!(
        "{}",
        format!("── 补退选课程 (第 {}/{} 页) ──", page + 1, total_pages).bold()
    );
    println!();

    if courses.is_empty() {
        println!("  {}", "本页无课程".dimmed());
        return;
    }

    for (i, c) in courses.iter().enumerate() {
        let idx = page * 20 + i + 1;
        let full_mark = if c.is_full() {
            " [满]".red().to_string()
        } else {
            String::new()
        };

        println!(
            "  {} {} {} {}{}",
            format!("[{}]", idx).cyan(),
            c.base.name.bold(),
            format!("{} 班号:{}", c.base.teacher, c.base.class_id).dimmed(),
            format!("({})", c.base.status).dimmed(),
            full_mark,
        );
        if !c.base.classroom.is_empty() {
            println!(
                "      {} {} | {} {}",
                "教室:".dimmed(),
                c.base.classroom,
                "学分:".dimmed(),
                c.base.credit,
            );
        }
    }
    println!();
}

/// 打印已选课程列表
pub fn print_elected(courses: &[CourseData]) {
    println!("{}", "── 已选课程 ──".bold());
    println!();

    if courses.is_empty() {
        println!("  {}", "暂无已选课程".dimmed());
        return;
    }

    for (i, c) in courses.iter().enumerate() {
        println!(
            "  {} {} {} {}",
            format!("[{}]", i + 1).cyan(),
            c.name.bold(),
            format!("{} 班号:{}", c.teacher, c.class_id).dimmed(),
            format!("({})", c.status).dimmed(),
        );
    }
    println!();
}

/// 打印自动选课配置列表
pub fn print_auto_elect_list(courses: &[AutoElectCourse]) {
    println!("{}", "── 自动选课目标 ──".bold());
    println!();

    if courses.is_empty() {
        println!("  {}", "未配置任何自动选课目标".dimmed());
        return;
    }

    for (i, c) in courses.iter().enumerate() {
        println!(
            "  {} {} - {} (班号: {}, 页: {})",
            format!("[{}]", i + 1).cyan(),
            c.name.bold(),
            c.teacher,
            c.class_id,
            c.page_id,
        );
    }
    println!();
}
