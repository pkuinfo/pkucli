//! CLI 命令处理

use crate::api::CardApi;
use crate::display;
use crate::login;
use anyhow::{anyhow, Context, Result};
use chrono::{Datelike, Local, NaiveDate};
use colored::Colorize;
use std::io::{self, Write};

/// 查看校园卡信息
pub async fn cmd_info() -> Result<()> {
    let jwt = login::load_jwt()?;
    let api = CardApi::new(&jwt)?;

    let card_data = api.query_card().await?;
    if card_data.card.is_empty() {
        println!("{} 未查到校园卡信息", "○".red());
        return Ok(());
    }

    for card in &card_data.card {
        display::print_card_info(card);
    }

    Ok(())
}

/// 显示付款码
pub async fn cmd_pay() -> Result<()> {
    let jwt = login::load_jwt()?;
    let api = CardApi::new(&jwt)?;

    // 先获取卡片信息确定 account
    let card_data = api.query_card().await?;
    let card = card_data
        .card
        .first()
        .ok_or_else(|| anyhow!("未查到校园卡信息"))?;

    let account = card
        .account
        .as_deref()
        .ok_or_else(|| anyhow!("校园卡缺少账户信息"))?;

    // 检查当前用卡方式，付款码需要「数字卡」(code=1)
    let card_config = api.get_use_card_config().await?;
    let current_type: i32 = card_config.card_type.parse().unwrap_or(-1);
    let current_name = card_config
        .card_types
        .iter()
        .find(|t| t.code == current_type)
        .map(|t| t.name.as_str())
        .unwrap_or("未知");

    if current_type != crate::api::CARD_TYPE_DIGITAL {
        println!();
        println!(
            "{} 当前用卡方式为「{}」，付款码需要切换到「数字卡」",
            "⚠".yellow().bold(),
            current_name.yellow(),
        );

        // 检查用卡冲突（是否允许切换）
        let conflict = api.get_conflict_info(account).await?;
        if conflict.status.as_deref() == Some("0") {
            let msg = conflict
                .message
                .as_deref()
                .unwrap_or("用卡方式冲突");
            let msg = regex::Regex::new(r"<[^>]+>|<!DOCTYPE[^>]*>")
                .unwrap()
                .replace_all(msg, "")
                .replace("&ldquo;", "\u{201C}")
                .replace("&rdquo;", "\u{201D}");
            println!("  {}", msg.trim().dimmed());
        }

        println!();
        print!("是否切换到数字卡并生成付款码？(y/N) ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("已取消");
            return Ok(());
        }

        // 切换到数字卡
        print!("  切换用卡方式...");
        io::stdout().flush()?;
        api.set_use_card_config(crate::api::CARD_TYPE_DIGITAL).await?;
        println!(" {}", "OK".green());
    }

    // 获取付款账户信息
    let pay_infos = api.get_pay_info().await?;
    let pay_info = pay_infos
        .first()
        .ok_or_else(|| anyhow!("未获取到付款信息"))?;

    let balance = pay_info.elec_accamt as f64 / 100.0;
    let name = &pay_info.name;
    let payacc = pay_info.payacc.as_deref().unwrap_or("001");
    let paytype = pay_info.paytype.as_deref().unwrap_or("1");

    // 获取实际的付款条码
    let barcode_data = api
        .get_barcode(account, payacc, paytype)
        .await?;

    let barcode = barcode_data
        .barcode
        .first()
        .ok_or_else(|| anyhow!("未获取到付款条码"))?;

    println!();
    println!("  {} [{}]", "付款码".bold(), name.cyan());
    println!("  账号: {}", account);
    println!("  余额: {} 元", format!("{balance:.2}").yellow());
    println!();

    // 渲染付款码 QR（与网页端一致：L 级纠错，无边距）
    render_qr_paycode(barcode)?;

    println!();
    println!("  {}", "将此二维码出示给 POS 机扫描即可付款".dimmed());
    println!(
        "  {}",
        "付款码每 60 秒刷新，如过期请重新运行 `campuscard pay`".dimmed()
    );
    println!();

    Ok(())
}

/// 充值
pub async fn cmd_recharge(amount: Option<f64>) -> Result<()> {
    let jwt = login::load_jwt()?;
    let api = CardApi::new(&jwt)?;

    // 获取卡片信息
    let card_data = api.query_card().await?;
    let card = card_data
        .card
        .first()
        .ok_or_else(|| anyhow!("未查到校园卡信息"))?;

    let account = card
        .account
        .as_deref()
        .ok_or_else(|| anyhow!("校园卡缺少账户信息"))?;

    let balance = card.elec_accamt as f64 / 100.0;

    println!();
    println!(
        "  当前余额: {} 元",
        format!("{balance:.2}").yellow().bold()
    );

    // 输入金额
    let amount = match amount {
        Some(a) => a,
        None => {
            println!("  可选金额: 10 / 50 / 100 / 200 / 500 / 800");
            print!("  充值金额(元): ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            input
                .trim()
                .parse::<f64>()
                .context("请输入有效金额")?
        }
    };

    if amount <= 0.0 {
        return Err(anyhow!("充值金额必须大于 0"));
    }

    let amount_yuan = amount.round() as i64;

    // 创建订单 → 获取收银台链接
    print!("  创建订单...");
    io::stdout().flush()?;
    let cashier_url = api
        .create_recharge_order(&jwt, account, amount_yuan)
        .await?;
    println!(" {}", "OK".green());

    println!();
    println!(
        "  {} 充值 {} 元",
        "订单已创建".green().bold(),
        format!("{amount_yuan}").cyan(),
    );
    println!();

    // 渲染收银台链接二维码（URL 较长，强制紧凑显示）
    render_qr_compact(&cashier_url)?;

    println!();
    println!("  {}", "用手机浏览器扫描上方二维码，选择支付方式完成支付".dimmed());
    println!(
        "  {}",
        "充值后需在 POS 机上刷一次卡才能到账".yellow()
    );
    println!();

    Ok(())
}

/// 查看交易记录
pub async fn cmd_bills(
    page: Option<usize>,
    size: Option<usize>,
    month: Option<&str>,
) -> Result<()> {
    let jwt = login::load_jwt()?;
    let api = CardApi::new(&jwt)?;

    let page = page.unwrap_or(1) as i64;
    let size = size.unwrap_or(10) as i64;

    // 解析月份筛选
    let (time_from, time_to) = if let Some(m) = month {
        let date = NaiveDate::parse_from_str(&format!("{m}-01"), "%Y-%m-%d")
            .context("月份格式错误，请使用 YYYY-MM")?;
        let last_day = last_day_of_month(date.year(), date.month());
        (Some(date), Some(last_day))
    } else {
        (None, None)
    };

    let data = api
        .get_turnovers(page, size, None, time_from.as_ref(), time_to.as_ref())
        .await?;

    display::print_turnovers(&data.records, data.current, data.pages, data.total);

    Ok(())
}

/// 查看统计信息
pub async fn cmd_stats(month: Option<&str>) -> Result<()> {
    let jwt = login::load_jwt()?;
    let api = CardApi::new(&jwt)?;

    let now = Local::now().date_naive();
    let month_str = month.unwrap_or(&now.format("%Y-%m").to_string()).to_string();

    let first_day = NaiveDate::parse_from_str(&format!("{month_str}-01"), "%Y-%m-%d")
        .context("月份格式错误，请使用 YYYY-MM")?;
    let last_day = last_day_of_month(first_day.year(), first_day.month());

    // 并行获取统计数据
    let (count, categories, daily) = tokio::try_join!(
        api.get_turnover_count(&first_day, &last_day),
        api.get_category_stats(&first_day, &last_day, 2), // type=2 支出
        api.get_daily_stats(&month_str, 2),
    )?;

    display::print_monthly_stats(&month_str, &count, &categories, &daily);

    Ok(())
}

/// 计算某月最后一天
fn last_day_of_month(year: i32, month: u32) -> NaiveDate {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .unwrap()
        .pred_opt()
        .unwrap()
}

/// 渲染付款码 QR（与网页端 vue-qr 一致：correctLevel=L，margin=0）
fn render_qr_paycode(content: &str) -> Result<()> {
    use qrcode::render::unicode::Dense1x2;
    use qrcode::EcLevel;

    let code = qrcode::QrCode::with_error_correction_level(content.as_bytes(), EcLevel::L)
        .map_err(|e| anyhow!("生成二维码失败: {e}"))?;
    let rendered = code
        .render::<Dense1x2>()
        .dark_color(Dense1x2::Light)
        .light_color(Dense1x2::Dark)
        .quiet_zone(false)
        .module_dimensions(2, 2)
        .build();
    println!("{rendered}");
    Ok(())
}

/// 紧凑渲染 QR 码（scale=1，无 quiet zone），适合长 URL
fn render_qr_compact(content: &str) -> Result<()> {
    use qrcode::render::unicode::Dense1x2;

    let code = qrcode::QrCode::new(content.as_bytes())
        .map_err(|e| anyhow!("生成二维码失败: {e}"))?;
    let rendered = code
        .render::<Dense1x2>()
        .dark_color(Dense1x2::Light)
        .light_color(Dense1x2::Dark)
        .quiet_zone(false)
        .module_dimensions(1, 1)
        .build();
    println!("{rendered}");
    Ok(())
}
