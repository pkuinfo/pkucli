//! HTTP 客户端构建

use reqwest::header::{HeaderMap, HeaderValue};
use reqwest_cookie_store::CookieStoreMutex;
use std::sync::Arc;

pub const DISK_BASE: &str = "https://disk.pku.edu.cn";

const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36";

fn headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("user-agent", HeaderValue::from_static(UA));
    headers.insert(
        "accept-language",
        HeaderValue::from_static("zh,zh-CN;q=0.9"),
    );
    headers
}

/// 带 cookie jar 的客户端（用于业务 API 和令牌续期）
pub fn build(cookie_store: Arc<CookieStoreMutex>) -> anyhow::Result<reqwest::Client> {
    let builder = reqwest::Client::builder()
        .cookie_provider(cookie_store)
        .default_headers(headers());
    Ok(pkuinfo_common::tls::apply_extra_roots(builder)?.build()?)
}

/// 带 cookie jar、不自动跟随重定向的客户端（OAuth2 登录链路要逐跳处理）
pub fn build_no_redirect(cookie_store: Arc<CookieStoreMutex>) -> anyhow::Result<reqwest::Client> {
    let builder = reqwest::Client::builder()
        .cookie_provider(cookie_store)
        .default_headers(headers())
        .redirect(reqwest::redirect::Policy::none());
    Ok(pkuinfo_common::tls::apply_extra_roots(builder)?.build()?)
}

/// 带内置 cookie 管理的客户端（用于 IAAA 登录）
pub fn build_simple() -> anyhow::Result<reqwest::Client> {
    let builder = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(headers());
    Ok(pkuinfo_common::tls::apply_extra_roots(builder)?.build()?)
}
