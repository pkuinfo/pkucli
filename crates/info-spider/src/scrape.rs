//! 把单篇文章 URL 抓取为 Markdown。
//!
//! 实现策略：用 reqwest 以 WeChat 内置浏览器的 UA 拉取文章 HTML，
//! 再用 html2md 转换为 markdown。这覆盖了 spider-rs/spider `Scrape` 单页场景的核心能力，
//! 且避免了 spider 整套爬虫框架带来的依赖负担。
//!
//! 对 mp.weixin.qq.com 的文章页面会做轻量清洗：
//!   * 去掉 <script> / <style> 块
//!   * 仅保留正文容器 `#js_content`（若存在）
//!
//! 这样 AI 拿到的 markdown 是干净的正文，不会夹杂导航、广告等噪声。

use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue};
use std::path::PathBuf;

pub const WX_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 16_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148 MicroMessenger/8.0.40(0x18002830) NetType/WIFI Language/zh_CN";

pub async fn run(url: String, output: Option<PathBuf>) -> Result<()> {
    let md = fetch_as_markdown(&url).await?;
    match output {
        Some(path) => {
            tokio::fs::write(&path, md.as_bytes())
                .await
                .with_context(|| format!("写入 {} 失败", path.display()))?;
            println!("✓ 已写入 {}", path.display());
        }
        None => {
            println!("{md}");
        }
    }
    Ok(())
}

pub async fn fetch_as_markdown(url: &str) -> Result<String> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "accept",
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        ),
    );
    headers.insert("accept-language", HeaderValue::from_static("zh-CN,zh;q=0.9"));

    let client = reqwest::Client::builder()
        .user_agent(WX_UA)
        .default_headers(headers)
        .gzip(true)
        .brotli(true)
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let html = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("请求 {url} 失败"))?
        .error_for_status()?
        .text()
        .await?;

    let cleaned = clean_wechat_html(&html);
    let md = html2md::parse_html(&cleaned);
    Ok(md)
}

/// 针对 mp.weixin.qq.com 文章页做最小清洗：
///   * 抽取 `<div id="js_content">...</div>` 正文块
///   * 去掉 script/style 标签
fn clean_wechat_html(html: &str) -> String {
    let stripped = strip_block(html, "<script", "</script>");
    let stripped = strip_block(&stripped, "<style", "</style>");

    if let Some(body) = extract_js_content(&stripped) {
        body
    } else {
        stripped
    }
}

fn extract_js_content(html: &str) -> Option<String> {
    let start_marker = "id=\"js_content\"";
    let idx = html.find(start_marker)?;
    // 向前找 `<div`
    let before = &html[..idx];
    let div_start = before.rfind("<div")?;

    // 从 div_start 开始做简单的嵌套匹配，找到对应 </div>
    let sub = &html[div_start..];
    let mut depth = 0i32;
    let bytes = sub.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            if sub[i..].starts_with("<div") {
                depth += 1;
                i += 4;
                continue;
            } else if sub[i..].starts_with("</div>") {
                depth -= 1;
                i += 6;
                if depth == 0 {
                    return Some(sub[..i].to_string());
                }
                continue;
            }
        }
        i += 1;
    }
    None
}

fn strip_block(input: &str, start_tag: &str, end_tag: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(s) = rest.find(start_tag) {
        out.push_str(&rest[..s]);
        if let Some(e) = rest[s..].find(end_tag) {
            rest = &rest[s + e + end_tag.len()..];
        } else {
            rest = "";
            break;
        }
    }
    out.push_str(rest);
    out
}
