//! HTTP 客户端构建

use reqwest::header::{HeaderMap, HeaderValue};
use reqwest_cookie_store::CookieStoreMutex;
use std::sync::Arc;

pub const ELECTIVE_BASE: &str = "https://elective.pku.edu.cn/elective2008";

/// SSO 登录 URL
pub const SSO_LOGIN: &str =
    "https://elective.pku.edu.cn/elective2008/ssoLogin.do";

/// IAAA OAuth redirect（注意：用 HTTP + 端口 80）
pub const OAUTH_REDIR: &str =
    "http://elective.pku.edu.cn:80/elective2008/ssoLogin.do";

/// 构建携带 cookie jar 的 reqwest 客户端（手动重定向，SSO 需要）
pub fn build(cookie_store: Arc<CookieStoreMutex>) -> anyhow::Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "accept",
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        ),
    );
    headers.insert(
        "user-agent",
        HeaderValue::from_static(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36",
        ),
    );

    let client = reqwest::Client::builder()
        .cookie_provider(cookie_store)
        .redirect(reqwest::redirect::Policy::none())
        .default_headers(headers)
        .build()?;
    Ok(client)
}

/// 构建简单客户端（IAAA 认证用）
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
