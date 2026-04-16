//! HTTP 客户端构建

use reqwest::header::{HeaderMap, HeaderValue};
use reqwest_cookie_store::CookieStoreMutex;
use std::sync::Arc;

pub const CWFW_BASE: &str = "https://cwfw.pku.edu.cn";
pub const CWFW_PROJECT: &str = "WF_CWBS";

const UA: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36";

/// 构建带 cookie jar 的客户端（用于 API 调用）
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

    let client = reqwest::Client::builder()
        .cookie_provider(cookie_store)
        .default_headers(headers)
        .build()?;
    Ok(client)
}

/// 构建带内置 cookie 管理的客户端（用于 IAAA 登录）
pub fn build_simple() -> anyhow::Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert("user-agent", HeaderValue::from_static(UA));

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(headers)
        .build()?;
    Ok(client)
}
