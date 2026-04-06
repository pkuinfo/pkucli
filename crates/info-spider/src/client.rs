use anyhow::{Context, Result};
use rand::Rng;
use reqwest::header::{self, HeaderMap, HeaderValue};
use reqwest::Client;
use reqwest_cookie_store::CookieStoreMutex;
use std::sync::Arc;
use std::time::Duration;

pub const BASE: &str = "https://mp.weixin.qq.com";
pub const DEFAULT_UA: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36";

/// 构建一个带 cookie 持久化 + 常用 headers 的 reqwest client。
pub fn build(cookie_store: Arc<CookieStoreMutex>) -> Result<Client> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::ACCEPT_LANGUAGE,
        HeaderValue::from_static("zh,zh-CN;q=0.9"),
    );
    headers.insert(
        header::ACCEPT,
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8",
        ),
    );

    let client = Client::builder()
        .user_agent(DEFAULT_UA)
        .default_headers(headers)
        .cookie_provider(cookie_store)
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(Duration::from_secs(30))
        .gzip(true)
        .brotli(true)
        .build()
        .context("构建 HTTP client 失败")?;

    Ok(client)
}

/// 构造一个 xhr 风格的 header 集合，用于访问 cgi-bin 接口。
/// `referer` 应设为编辑器页面 URL 或 `BASE`。
pub fn xhr_headers(referer: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(
        "x-requested-with",
        HeaderValue::from_static("XMLHttpRequest"),
    );
    h.insert(header::REFERER, HeaderValue::from_str(referer).unwrap());
    h.insert(
        header::ACCEPT,
        HeaderValue::from_static("*/*"),
    );
    h.insert(
        "sec-fetch-dest",
        HeaderValue::from_static("empty"),
    );
    h.insert(
        "sec-fetch-mode",
        HeaderValue::from_static("cors"),
    );
    h.insert(
        "sec-fetch-site",
        HeaderValue::from_static("same-origin"),
    );
    h
}

/// 睡眠一个带抖动的随机时间，默认 base_ms * (1 ± 0.5)。
pub async fn jitter_sleep(base_ms: u64) {
    let jitter: f64 = {
        let mut rng = rand::thread_rng();
        rng.gen_range(0.5..1.5)
    };
    let ms = (base_ms as f64 * jitter) as u64;
    tokio::time::sleep(Duration::from_millis(ms)).await;
}
