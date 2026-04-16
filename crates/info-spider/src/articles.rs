//! 列出公众号文章。调用 `/cgi-bin/appmsgpublish`。
//!
//! 响应是三层嵌套 JSON：
//!   外层           { base_resp, publish_page: "<str>" }
//!     publish_page { publish_list: [ { publish_info: "<str>" } ], total_count }
//!       publish_info { appmsgex: [ { title, link, cover, update_time, ... } ] }

use crate::{
    client::{xhr_headers, BASE},
    search::{self, BizItem, Ctx},
};
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Article {
    pub aid: String,
    pub appmsgid: i64,
    pub itemidx: i64,
    pub title: String,
    pub link: String,
    pub cover: String,
    pub digest: String,
    pub author_name: String,
    pub update_time: i64,
    pub create_time: i64,
}

#[derive(Debug)]
pub struct PageResult {
    pub articles: Vec<Article>,
    pub total_count: i64,
}

pub async fn fetch_page(ctx: &Ctx, fakeid: &str, begin: u32, count: u32) -> Result<PageResult> {
    let url = format!(
        "{BASE}/cgi-bin/appmsgpublish?sub=list&search_field=null&begin={begin}&count={count}&query=&fakeid={}&type=101_1&free_publish_type=1&sub_action=list_ex&fingerprint={}&token={}&lang=zh_CN&f=json&ajax=1",
        urlencoding::encode(fakeid),
        ctx.fingerprint,
        ctx.token,
    );
    let resp: Value = ctx
        .client
        .get(&url)
        .headers(xhr_headers(&ctx.editor_referer))
        .send()
        .await
        .context("appmsgpublish 请求失败")?
        .error_for_status()?
        .json()
        .await?;

    let ret = resp
        .pointer("/base_resp/ret")
        .and_then(|v| v.as_i64())
        .unwrap_or(-1);
    if ret != 0 {
        let msg = resp
            .pointer("/base_resp/err_msg")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        if ret == 200003 {
            return Err(anyhow!("session 已失效，请重新 login"));
        }
        if ret == 200013 {
            return Err(anyhow!("触发频率限制 (ret=200013)，请稍后再试或放慢速度"));
        }
        return Err(anyhow!("appmsgpublish 失败 ret={ret} err_msg={msg}"));
    }

    // publish_page 是字符串化的 JSON
    let publish_page_str = resp
        .get("publish_page")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("响应缺少 publish_page 字段"))?;
    let publish_page: Value =
        serde_json::from_str(publish_page_str).context("解析 publish_page 字符串失败")?;

    let total_count = publish_page
        .get("total_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let mut articles: Vec<Article> = vec![];
    if let Some(list) = publish_page.get("publish_list").and_then(|v| v.as_array()) {
        for item in list {
            let Some(info_str) = item.get("publish_info").and_then(|v| v.as_str()) else {
                continue;
            };
            let info: Value = match serde_json::from_str(info_str) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let Some(appmsgex) = info.get("appmsgex").and_then(|v| v.as_array()) else {
                continue;
            };
            for a in appmsgex {
                if let Ok(parsed) = parse_article(a) {
                    articles.push(parsed);
                }
            }
        }
    }
    Ok(PageResult {
        articles,
        total_count,
    })
}

fn parse_article(v: &Value) -> Result<Article> {
    let get_str =
        |k: &str| -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() };
    let get_i64 = |k: &str| -> i64 { v.get(k).and_then(|x| x.as_i64()).unwrap_or(0) };
    Ok(Article {
        aid: get_str("aid"),
        appmsgid: get_i64("appmsgid"),
        itemidx: get_i64("itemidx"),
        title: get_str("title"),
        link: get_str("link"),
        cover: get_str("cover"),
        digest: get_str("digest"),
        author_name: get_str("author_name"),
        update_time: get_i64("update_time"),
        create_time: get_i64("create_time"),
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    name: Option<String>,
    fakeid_arg: Option<String>,
    begin: u32,
    count: u32,
    limit: Option<u32>,
    delay_ms: u64,
    format: String,
) -> Result<()> {
    let ctx = search::warmup().await?;

    // 先拿到 fakeid
    let (fakeid, biz_nick): (String, Option<String>) = if let Some(f) = fakeid_arg {
        (f, None)
    } else if let Some(q) = name {
        let list = search::search_biz(&ctx, &q, 5).await?;
        let first: BizItem = list
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("未找到匹配公众号: {q}"))?;
        println!(
            "{} 命中公众号: {} ({})  fakeid={}",
            "[info]".cyan(),
            first.nickname.bold(),
            first.alias.dimmed(),
            first.fakeid.green()
        );
        search::polite_pause().await;
        (first.fakeid.clone(), Some(first.nickname))
    } else {
        return Err(anyhow!("必须指定 --name 或 --fakeid"));
    };

    let target = limit.unwrap_or(count);
    let mut collected: Vec<Article> = vec![];
    let mut cur = begin;
    let mut total_hint: i64 = 0;

    while (collected.len() as u32) < target {
        let page = fetch_page(&ctx, &fakeid, cur, count).await?;
        total_hint = page.total_count;
        if page.articles.is_empty() {
            break;
        }
        let need = (target as usize).saturating_sub(collected.len());
        let slice: Vec<Article> = page.articles.iter().take(need).cloned().collect();
        let got = slice.len();
        collected.extend(slice);

        if got < count as usize {
            break; // 没有更多
        }
        cur += count;

        // 人类节奏：翻页延迟 + 抖动
        let jitter: f64 = {
            use rand::Rng;
            rand::thread_rng().gen_range(1.0..1.5)
        };
        let ms = (delay_ms as f64 * jitter) as u64;
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    }

    // 保存 cookie
    let store = crate::session::Store::new()?;
    store.save_cookie_store(&store.load_cookie_store()?)?;

    match format.as_str() {
        "json" => println!("{}", serde_json::to_string_pretty(&collected)?),
        "jsonl" => {
            for a in &collected {
                println!("{}", serde_json::to_string(a)?);
            }
        }
        _ => {
            if let Some(n) = biz_nick {
                println!(
                    "\n{} {} （共 {} 篇，命中 {} 篇）",
                    "●".green(),
                    n.bold(),
                    total_hint,
                    collected.len()
                );
            }
            for (i, a) in collected.iter().enumerate() {
                let date = chrono::DateTime::from_timestamp(a.update_time, 0)
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|| "-".into());
                println!(
                    "  {}. [{}] {}",
                    (i + 1).to_string().cyan(),
                    date.dimmed(),
                    a.title.bold()
                );
                println!("      {}", a.link.blue().underline());
            }
        }
    }
    Ok(())
}
