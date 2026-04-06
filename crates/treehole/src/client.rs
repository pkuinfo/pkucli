//! HTTP 客户端构建与公共请求头

use reqwest::header::{HeaderMap, HeaderValue};
use reqwest_cookie_store::CookieStoreMutex;
use std::sync::Arc;

pub const TREEHOLE_BASE: &str = "https://treehole.pku.edu.cn";

/// 构建携带 cookie jar 的 reqwest 客户端
pub fn build(cookie_store: Arc<CookieStoreMutex>) -> anyhow::Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("application/json, text/plain, */*"));
    headers.insert(
        "user-agent",
        HeaderValue::from_static(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36",
        ),
    );

    let client = reqwest::Client::builder()
        .cookie_provider(cookie_store)
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::none()) // 手动处理重定向
        .build()?;
    Ok(client)
}

/// 构建带内置 cookie 管理的客户端（用于 IAAA 认证，需要 JSESSIONID）
pub fn build_simple() -> anyhow::Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "user-agent",
        HeaderValue::from_static(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36",
        ),
    );

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(headers)
        .build()?;
    Ok(client)
}
