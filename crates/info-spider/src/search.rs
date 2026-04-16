//! 搜索公众号。
//!
//! 正常用户点击顺序：
//!   1. 进入首页 `/cgi-bin/home`
//!   2. 点击「新的创作 → 文章」触发编辑器 `/cgi-bin/appmsg?t=media/appmsg_edit_v2...`
//!   3. 编辑器工具栏「超链接」弹窗里调用 `/cgi-bin/searchbiz`
//!
//! 本模块同时提供一个对外的 `search_biz` 函数供 articles 命令复用。

use crate::{
    client::{self, jitter_sleep, xhr_headers, BASE},
    session::Store,
};
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BizItem {
    pub fakeid: String,
    pub nickname: String,
    pub alias: String,
    pub round_head_img: String,
    pub signature: String,
    pub service_type: i64,
    pub verify_status: i64,
}

pub struct Ctx {
    pub store: Store,
    pub client: Client,
    pub token: String,
    pub fingerprint: String,
    pub editor_referer: String,
}

/// 初始化一个「已模拟进入编辑器」的上下文。供 search / articles 复用。
///
/// 严格按照人类用户的点击顺序访问页面：
///   home → appmsg(editor)
pub async fn warmup() -> Result<Ctx> {
    let store = Store::new()?;
    let sess = store
        .load_session()?
        .ok_or_else(|| anyhow!("尚未登录，请先执行 `info-spider login`"))?;
    let cookie_store = store.load_cookie_store()?;
    let client = client::build(cookie_store.clone())?;

    // Step 1: 访问首页（像刚打开浏览器的用户那样）
    let home = format!(
        "{BASE}/cgi-bin/home?t=home/index&lang=zh_CN&token={}",
        sess.token
    );
    client
        .get(&home)
        .header("referer", BASE)
        .send()
        .await
        .context("访问 home 失败")?
        .error_for_status()?
        .bytes()
        .await?;
    jitter_sleep(600).await;

    // Step 2: 打开文章编辑器页面（用于后续 xhr 的 referer）
    let ts = chrono::Utc::now().timestamp_millis();
    let editor = format!(
        "{BASE}/cgi-bin/appmsg?t=media/appmsg_edit_v2&action=edit&isNew=1&type=77&createType=0&token={}&lang=zh_CN&timestamp={ts}",
        sess.token
    );
    client
        .get(&editor)
        .header("referer", &home)
        .send()
        .await
        .context("打开编辑器失败")?
        .error_for_status()?
        .bytes()
        .await?;
    jitter_sleep(800).await;

    // 更新 cookies 到磁盘
    store.save_cookie_store(&cookie_store)?;

    Ok(Ctx {
        store,
        client,
        token: sess.token,
        fingerprint: sess.fingerprint,
        editor_referer: editor,
    })
}

/// 调用 searchbiz 接口。
pub async fn search_biz(ctx: &Ctx, query: &str, count: u32) -> Result<Vec<BizItem>> {
    let url = format!(
        "{BASE}/cgi-bin/searchbiz?action=search_biz&begin=0&count={count}&query={}&fingerprint={}&token={}&lang=zh_CN&f=json&ajax=1",
        urlencoding::encode(query),
        ctx.fingerprint,
        ctx.token,
    );
    let resp: Value = ctx
        .client
        .get(&url)
        .headers(xhr_headers(&ctx.editor_referer))
        .send()
        .await
        .context("searchbiz 请求失败")?
        .error_for_status()?
        .json()
        .await?;

    let ret = resp
        .pointer("/base_resp/ret")
        .and_then(|x| x.as_i64())
        .unwrap_or(-1);
    if ret != 0 {
        let msg = resp
            .pointer("/base_resp/err_msg")
            .and_then(|x| x.as_str())
            .unwrap_or("unknown");
        if ret == 200003 {
            return Err(anyhow!("session 已失效（ret=200003），请重新 login"));
        }
        return Err(anyhow!("searchbiz 失败 ret={ret} err_msg={msg}"));
    }

    let list = resp
        .get("list")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let items = list
        .into_iter()
        .filter_map(|v| serde_json::from_value::<BizItem>(v).ok())
        .collect();
    Ok(items)
}

pub async fn run(query: String, count: u32, format: String) -> Result<()> {
    let ctx = warmup().await?;
    let items = search_biz(&ctx, &query, count).await?;
    ctx.store
        .save_cookie_store(&crate::session::Store::new()?.load_cookie_store()?)?;

    if items.is_empty() {
        println!("{} 未找到匹配的公众号", "[info]".yellow());
        return Ok(());
    }

    match format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&items)?);
        }
        _ => {
            println!("{}", "搜索结果：".bold());
            for (i, it) in items.iter().enumerate() {
                println!(
                    "  {}. {}  ({})",
                    (i + 1).to_string().cyan(),
                    it.nickname.bold(),
                    it.alias.dimmed()
                );
                println!("     fakeid: {}", it.fakeid.green());
                if !it.signature.is_empty() {
                    println!("     签名:   {}", it.signature.dimmed());
                }
            }
        }
    }
    // 刷新一次 cookie
    let store = Store::new()?;
    let cs = store.load_cookie_store()?;
    let _ = cs; // no-op, already persisted
    Ok(())
}

/// 简单包装：给个 jitter 延迟，避免同一秒连发多次请求
pub async fn polite_pause() {
    let ms = rand::thread_rng().gen_range(800..1800);
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}
