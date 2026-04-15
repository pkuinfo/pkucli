//! portal 相关 reqwest 客户端
//!
//! portal 的三项功能访问不同的 host：
//! - 空闲教室：portal.pku.edu.cn/publicQuery（无需登录）
//! - 校历：simso.pku.edu.cn（无需登录，Vue SPA）
//! - 网费：its.pku.edu.cn（上网账号密码，有 cookie session）

use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest_cookie_store::CookieStoreMutex;
use std::sync::Arc;

const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
                  (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36";

pub const ITS_BASE: &str = "https://its.pku.edu.cn";
pub const PORTAL_PUBLIC_BASE: &str = "https://portal.pku.edu.cn/publicQuery";
pub const SIMSO_BASE: &str = "https://simso.pku.edu.cn";

/// 建立一个不带 cookie 的简单客户端——空闲教室 / 校历 都够用
pub fn build_simple() -> Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "accept-language",
        HeaderValue::from_static("zh,zh-CN;q=0.9"),
    );
    reqwest::Client::builder()
        .user_agent(UA)
        .default_headers(headers)
        .build()
        .context("构建 reqwest client 失败")
}

/// 带 cookie store 的客户端，用于 its.pku.edu.cn 的登录态请求
pub fn build_with_cookies(cookie_store: Arc<CookieStoreMutex>) -> Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "accept-language",
        HeaderValue::from_static("zh,zh-CN;q=0.9"),
    );
    reqwest::Client::builder()
        .user_agent(UA)
        .default_headers(headers)
        .cookie_provider(cookie_store)
        .build()
        .context("构建 reqwest client 失败")
}
