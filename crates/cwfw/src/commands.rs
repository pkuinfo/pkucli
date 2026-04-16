//! 命令处理器

use crate::{
    api::CwfwApi,
    client, context, display,
    login::{self, APP_NAME},
};
use anyhow::{anyhow, Result};
use colored::Colorize;
use pkuinfo_common::session::Store;

/// `cwfw reward` —— 个人酬金查询
///
/// - `year`:      年份，默认当前年
/// - `month_from` / `month_to`: 起止月份，默认 1 月到当前月
pub async fn cmd_reward(
    year: Option<u32>,
    month_from: Option<u32>,
    month_to: Option<u32>,
) -> Result<()> {
    let session = login::load_session()?;
    let uid = session
        .uid
        .clone()
        .ok_or_else(|| anyhow!("会话中缺少 uid，请重新 `cwfw login`"))?;

    let store = Store::new(APP_NAME)?;
    let cookie_store = store.load_cookie_store()?;
    let http = client::build(cookie_store.clone())?;

    // 构建 userContext：先从 loadRolesMenu 拉初始值，再合并 loadInitFunction 的用户变量，
    // 最后塞进 "个人酬金查询" 的静态字段。
    println!("{} 初始化会话上下文...", "[*]".cyan());
    let mut ctx = match context::fetch_user_context(&http).await {
        Ok(c) => c,
        Err(e) => {
            // 会话可能过期，提示用户重登
            return Err(anyhow!("{e}。会话可能已过期，请重新 `cwfw login`"));
        }
    };
    let _ = context::load_init_function(&http, &mut ctx).await; // 失败不致命
    context::seed_reward_query_context(&mut ctx, &uid);

    // 解析默认年份 / 月份
    let now = chrono::Local::now();
    let year = year.unwrap_or_else(|| now.format("%Y").to_string().parse().unwrap_or(2025));
    let month_from = month_from.unwrap_or(1);
    let month_to = month_to.unwrap_or_else(|| now.format("%m").to_string().parse().unwrap_or(12));

    validate_month(month_from, "起始月份")?;
    validate_month(month_to, "结束月份")?;
    if month_from > month_to {
        return Err(anyhow!("起始月份 {month_from} 不能大于结束月份 {month_to}"));
    }

    // 保存更新后的 cookies（loadRolesMenu 可能刷新过）
    let _ = store.save_cookie_store(&cookie_store);

    let api = CwfwApi::new(http, ctx, uid);

    println!(
        "{} 查询 {}年{:02}月-{:02}月 个人酬金...",
        "[*]".cyan(),
        year,
        month_from,
        month_to
    );
    let resp = api.query_reward(year, month_from, month_to).await?;

    display::render_reward_query(&resp, year, month_from, month_to);
    Ok(())
}

fn validate_month(month: u32, label: &str) -> Result<()> {
    if !(1..=12).contains(&month) {
        return Err(anyhow!("{label} {month} 不在 1-12 范围内"));
    }
    Ok(())
}
