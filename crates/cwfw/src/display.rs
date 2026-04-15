//! 结果渲染

use crate::api::{DoQueryResp, DoQueryRow};
use colored::Colorize;

/// 个人酬金查询返回的列顺序
const REWARD_COLUMNS: &[&str] = &[
    "摘要",
    "项目代码",
    "发放类型",
    "发放金额",
    "扣税金额",
    "实发金额",
    "录入时间",
    "发放时间",
    "经办人",
    "录入人",
    "发放部门",
    "发放班组",
];

/// 渲染个人酬金查询结果
pub fn render_reward_query(resp: &DoQueryResp, year: u32, month_from: u32, month_to: u32) {
    println!();
    println!(
        "{} {}年{:02}月 - {:02}月 个人酬金查询",
        "═══".cyan(),
        year,
        month_from,
        month_to
    );
    println!();

    if resp.rows.is_empty() {
        println!("{} 未查询到记录", "○".yellow());
        return;
    }

    // 最后一行通常是 "总计"，单独显示
    let (data_rows, total_row) = split_total(&resp.rows);

    for (idx, row) in data_rows.iter().enumerate() {
        println!(
            "{} {}",
            format!("[{}]", idx + 1).dimmed(),
            cell(row, 0).bold().cyan()
        );
        println!(
            "    {}  {}  {}",
            format!("项目代码: {}", cell(row, 1)).dimmed(),
            format!("发放类型: {}", cell(row, 2)).dimmed(),
            format!("发放时间: {}", cell(row, 7)).dimmed()
        );
        println!(
            "    {}  {}  {}",
            colored_amount("发放金额", &cell(row, 3), "white"),
            colored_amount("扣税金额", &cell(row, 4), "yellow"),
            colored_amount("实发金额", &cell(row, 5), "green")
        );
        println!(
            "    {}  {}  {}",
            format!("经办人: {}", cell(row, 8)).dimmed(),
            format!("发放部门: {}", cell(row, 10)).dimmed(),
            format!("录入时间: {}", cell(row, 6)).dimmed()
        );
        println!();
    }

    if let Some(total) = total_row {
        println!("{}", "─".repeat(60).dimmed());
        println!(
            "{}: 发放 {}  扣税 {}  实发 {}",
            "总计".bold(),
            cell(total, 3).bold(),
            cell(total, 4).bold().yellow(),
            cell(total, 5).bold().green()
        );
        println!(
            "共 {} 条记录",
            format!("{}", data_rows.len()).bold()
        );
    }
}

/// 列出字段名（调试用）
#[allow(dead_code)]
pub fn column_name(index: usize) -> &'static str {
    REWARD_COLUMNS.get(index).copied().unwrap_or("?")
}

fn cell(row: &DoQueryRow, index: usize) -> String {
    row.cell.get(index).cloned().unwrap_or_default()
}

fn colored_amount(label: &str, value: &str, color: &str) -> String {
    let text = format!("{label}: ¥{value}");
    match color {
        "green" => text.green().to_string(),
        "yellow" => text.yellow().to_string(),
        _ => text.to_string(),
    }
}

fn split_total(rows: &[DoQueryRow]) -> (&[DoQueryRow], Option<&DoQueryRow>) {
    match rows.last() {
        Some(last) if cell(last, 0) == "总计" || cell(last, 0).contains("总计") => {
            (&rows[..rows.len() - 1], Some(last))
        }
        _ => (rows, None),
    }
}
