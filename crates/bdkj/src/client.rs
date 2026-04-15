//! HTTP 客户端构建

use reqwest::header::{HeaderMap, HeaderValue};
use reqwest_cookie_store::CookieStoreMutex;
use std::sync::Arc;

pub const BDKJ_BASE: &str = "https://bdkj.pku.edu.cn";

const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36";

/// 带 cookie jar 的客户端（用于业务 API）
pub fn build(cookie_store: Arc<CookieStoreMutex>) -> anyhow::Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert("user-agent", HeaderValue::from_static(UA));
    headers.insert(
        "accept",
        HeaderValue::from_static("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
    );
    headers.insert(
        "accept-language",
        HeaderValue::from_static("zh,zh-CN;q=0.9"),
    );
    headers.insert(
        "referer",
        HeaderValue::from_static("https://bdkj.pku.edu.cn/"),
    );

    let client = reqwest::Client::builder()
        .cookie_provider(cookie_store)
        .default_headers(headers)
        .build()?;
    Ok(client)
}

/// 带内置 cookie 管理的客户端（用于 IAAA 登录）
pub fn build_simple() -> anyhow::Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert("user-agent", HeaderValue::from_static(UA));

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(headers)
        .build()?;
    Ok(client)
}
