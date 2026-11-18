//! HTTP 客户端构建

use reqwest::header::{HeaderMap, HeaderValue};
use reqwest_cookie_store::CookieStoreMutex;
use std::sync::Arc;

const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36";

fn headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("user-agent", HeaderValue::from_static(UA));
    headers.insert(
        "accept-language",
        HeaderValue::from_static("zh-CN,zh;q=0.9"),
    );
    headers
}

/// 带 cookie jar、不自动跟随重定向的客户端。
///
/// 登录链路要逐跳读 `Location`；邮件 RPC 遇到 302 说明会话失效，也不能跟随。
pub fn build_no_redirect(cookie_store: Arc<CookieStoreMutex>) -> anyhow::Result<reqwest::Client> {
    let builder = reqwest::Client::builder()
        .cookie_provider(cookie_store)
        .default_headers(headers())
        .redirect(reqwest::redirect::Policy::none());
    Ok(pkuinfo_common::tls::apply_extra_roots(builder)?.build()?)
}
