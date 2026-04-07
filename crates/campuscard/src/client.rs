//! HTTP 客户端构建

use reqwest::header::{HeaderMap, HeaderValue};

pub const CARD_BASE: &str = "https://bdcard.pku.edu.cn";
pub const PORTAL_BASE: &str = "https://portal.pku.edu.cn/portal2017";

/// 移动端 User-Agent（校园卡服务要求移动端 UA）
const MOBILE_UA: &str =
    "PKUANDROID2.2.0_SM-S938B Dalvik/2.1.0 (Linux; U; Android 15; SM-S938B Build/BP1A.250305.020) okhttp/4.12.0";

/// 构建不跟随重定向的客户端（用于门户 SSO → 校园卡 JWT 流程，需要手动处理 302）
pub fn build_no_redirect() -> anyhow::Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "user-agent",
        HeaderValue::from_static(MOBILE_UA),
    );
    headers.insert(
        "x-requested-with",
        HeaderValue::from_static("cn.edu.pku.PKUAndroid"),
    );

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    Ok(client)
}

/// 构建简单客户端（用于 IAAA 认证）
pub fn build_simple() -> anyhow::Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "user-agent",
        HeaderValue::from_static(MOBILE_UA),
    );
    headers.insert(
        "x-requested-with",
        HeaderValue::from_static("cn.edu.pku.PKUAndroid"),
    );

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(headers)
        .build()?;
    Ok(client)
}

/// 构建带 JWT 认证头的 API 客户端（自动跟随重定向）
pub fn build_api(jwt: &str) -> anyhow::Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "user-agent",
        HeaderValue::from_static(MOBILE_UA),
    );
    headers.insert(
        "x-requested-with",
        HeaderValue::from_static("cn.edu.pku.PKUAndroid"),
    );
    headers.insert(
        "synjones-auth",
        HeaderValue::from_str(&format!("bearer {jwt}"))?,
    );
    headers.insert(
        "synaccesssource",
        HeaderValue::from_static("h5"),
    );

    let client = reqwest::Client::builder()
        .default_headers(headers)
        .http1_only()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    Ok(client)
}
