//! 终端输出格式化

use crate::api::{AccInfo, CardInfo, Turnover, TurnoverCategory, TurnoverCount};
use colored::Colorize;
use std::collections::HashMap;

/// 格式化金额（元，f64），带颜色
fn fmt_yuan(yuan: f64, is_expense: bool) -> String {
    if is_expense {
        format!("{yuan:.2}").red().to_string()
    } else {
        format!("{yuan:.2}").green().to_string()
    }
}

/// 打印校园卡信息
pub fn print_card_info(card: &CardInfo) {
    let name = card.name.as_deref().unwrap_or("未知").trim();
    let sno = card.sno.as_deref().unwrap_or("-");
    let account = card.account.as_deref().unwrap_or("-");
    let card_name = card.cardname.as_deref().unwrap_or("-");
    let balance = card.elec_accamt as f64 / 100.0;

    // 状态
    let status = if card.lostflag != 0 {
        "已挂失".red().bold().to_string()
    } else if card.freezeflag != 0 {
        "已冻结".yellow().bold().to_string()
    } else {
        "正常".green().bold().to_string()
    };

    let expdate = card
        .expdate
        .as_deref()
        .map(|d| {
            if d.len() == 8 {
                format!("{}-{}-{}", &d[..4], &d[4..6], &d[6..8])
            } else {
                d.to_string()
            }
        })
        .unwrap_or_else(|| "-".into());

    println!();
    println!(
        "  {} {} {}",
        "校园卡".bold(),
        format!("({card_name})").dimmed(),
        status,
    );
    println!("  {}", "─".repeat(40).dimmed());
    println!("  姓名     {}", name.bold());
    println!("  学工号   {sno}");
    println!("  卡号     {account}");
    println!("  到期日   {expdate}");
    println!("  总余额   {} 元", format!("{balance:.2}").yellow().bold());

    if let Some(accounts) = &card.accinfo {
        println!();
        println!("  {}", "账户明细".bold());
        println!("  {}", "─".repeat(40).dimmed());
        for acc in accounts {
            print_acc_info(acc);
        }
    }
    println!();
}

fn print_acc_info(acc: &AccInfo) {
    let name = acc.name.as_deref().unwrap_or("未知账户");
    let acc_type = acc.acc_type.as_deref().unwrap_or("-");
    let balance = acc.balance as f64 / 100.0;

    println!(
        "  {} [{}]  {} 元",
        name.cyan(),
        acc_type.dimmed(),
        format!("{balance:.2}").yellow(),
    );

    if let Some(day_cost) = acc.daycostamt {
        let day_limit = acc.daycostlimit.unwrap_or(0) as f64 / 100.0;
        let day_cost_yuan = day_cost as f64 / 100.0;
        println!("    今日消费 {:.2} / {:.2} 元", day_cost_yuan, day_limit);
    }
}

/// 打印交易记录列表
pub fn print_turnovers(turnovers: &[Turnover], page: i64, total_pages: i64, total: i64) {
    if turnovers.is_empty() {
        println!("  {}", "暂无交易记录".dimmed());
        return;
    }

    println!();
    println!(
        "  {} ({}/{}页, 共{}条)",
        "交易记录".bold(),
        page,
        total_pages,
        total
    );
    println!("  {}", "─".repeat(70).dimmed());

    for t in turnovers {
        let desc = t.resume.as_deref().unwrap_or("未知交易");
        let ttype = t.turnover_type.as_deref().unwrap_or("");
        let time = t.effectdate_str.as_deref().unwrap_or("-");
        let balance = t.card_balance as f64 / 100.0;

        let is_income = t.icon.as_deref() == Some("recharge")
            || t.icon.as_deref() == Some("subsidy")
            || t.icon.as_deref() == Some("refund");

        let amount = if is_income {
            format!("+{:.2}", t.tranamt as f64 / 100.0)
                .green()
                .to_string()
        } else {
            format!("-{:.2}", t.tranamt as f64 / 100.0)
                .red()
                .to_string()
        };

        let type_badge = match t.icon.as_deref() {
            Some("consume") => "消费".red(),
            Some("recharge") => "充值".green(),
            Some("refund") => "退款".cyan(),
            Some("subsidy") => "补助".blue(),
            _ => ttype.normal(),
        };

        println!("  {} {:<6} {}", time.dimmed(), type_badge, amount,);
        println!("  {}  余额: {:.2}", desc.bold(), balance,);
        println!("  {}", "─".repeat(70).dimmed());
    }
}

/// 打印月度统计
pub fn print_monthly_stats(
    month: &str,
    count: &TurnoverCount,
    categories: &[TurnoverCategory],
    daily: &HashMap<String, f64>,
) {
    let income = count.income / 100.0;
    let expenses = count.expenses / 100.0;

    println!();
    println!("  {} {}", "月度统计".bold(), month.cyan());
    println!("  {}", "═".repeat(50).dimmed());
    println!(
        "  收入  {}  │  支出  {}",
        fmt_yuan(income, false),
        fmt_yuan(expenses, true),
    );
    println!("  {}", "─".repeat(50).dimmed());

    // 分类统计
    if !categories.is_empty() {
        println!("  {}", "支出分类".bold());
        for cat in categories {
            let name = cat.turnover_type.as_deref().unwrap_or("其他");
            let amount = cat.amount / 100.0;
            println!("    {:<10} {}", name, fmt_yuan(amount, true));
        }
        println!("  {}", "─".repeat(50).dimmed());
    }

    // 日度消费柱状图
    print_daily_chart(daily);
}

/// 打印日度消费迷你柱状图
fn print_daily_chart(daily: &HashMap<String, f64>) {
    if daily.is_empty() {
        return;
    }

    let mut entries: Vec<(&str, f64)> = daily
        .iter()
        .map(|(k, v)| (k.as_str(), *v / 100.0))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));

    // 只显示有消费的日期
    let active: Vec<_> = entries.iter().filter(|(_, v)| *v > 0.0).collect();
    if active.is_empty() {
        return;
    }

    let max_val = active.iter().map(|(_, v)| *v).fold(0.0_f64, f64::max);
    let bar_max = 30;

    println!("  {}", "每日支出".bold());
    for (date, val) in &active {
        let day = date.rsplit('-').next().unwrap_or(date);
        let bar_len = if max_val > 0.0 {
            (val / max_val * bar_max as f64).round() as usize
        } else {
            0
        };
        let bar: String = "█".repeat(bar_len);
        println!("  {} {} {:.2}", day.dimmed(), bar.red(), val,);
    }
    println!();
}
