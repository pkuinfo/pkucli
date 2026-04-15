//! 终端显示

use crate::model::CourseInfo;
use colored::Colorize;

/// 打印课程列表
pub fn print_courses(courses: &[CourseInfo]) {
    if courses.is_empty() {
        println!("{} 无课程数据", "○".red());
        return;
    }

    println!(
        "\n{} 共 {} 门课程\n",
        "●".green(),
        courses.len().to_string().bold()
    );

    for (i, c) in courses.iter().enumerate() {
        let idx = format!("[{:>3}]", i + 1).cyan();
        let name = c.course_name.bold();
        let teacher = if c.teacher.is_empty() {
            String::new()
        } else {
            format!(" {}", c.teacher)
        };
        let class_no = if c.class_no.is_empty() {
            String::new()
        } else {
            format!(" #{}", c.class_no)
        };

        println!(
            "{idx} {name}{teacher}{class_no}  ({})",
            c.category.dimmed()
        );

        // 第二行：详细信息
        let mut details = Vec::new();
        if !c.course_id.is_empty() {
            details.push(c.course_id.clone());
        }
        if !c.credit.is_empty() {
            details.push(format!("{}学分", c.credit));
        }
        if !c.department.is_empty() {
            details.push(c.department.clone());
        }
        if !details.is_empty() {
            println!("      {}", details.join(" | ").dimmed());
        }

        // 第三行：时间地点
        if !c.schedule.is_empty() || !c.classroom.is_empty() {
            let mut time_loc = Vec::new();
            if !c.weeks.is_empty() {
                time_loc.push(c.weeks.clone());
            }
            if !c.schedule.is_empty() {
                time_loc.push(c.schedule.clone());
            }
            if !c.classroom.is_empty() {
                time_loc.push(format!("📍{}", c.classroom));
            }
            println!("      {}", time_loc.join("  "));
        }

        // 第四行：备注（如果有）
        if !c.remark.is_empty() {
            let remark = if c.remark.len() > 80 {
                format!("{}...", &c.remark[..c.remark.floor_char_boundary(77)])
            } else {
                c.remark.clone()
            };
            println!("      {}", format!("备注: {remark}").dimmed());
        }
    }

    println!();
}
