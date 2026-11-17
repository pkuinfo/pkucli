//! 北大网盘登录流程
//!
//! disk.pku.edu.cn 是爱数 AnyShare 7，网页端走 OAuth2（Ory Hydra），第三方认证模块 PkuAuth 对接 IAAA：
//! 1. GET `/anyshare/oauth2/login?redirect=/anyshare/` → `/oauth2/auth` → `/oauth2/signin?login_challenge=C`
//!    （redirect 决定 state；state 不是以 / 开头的路径时，回调会 400「state验证失败」）
//! 2. 把 C 写进 `login_challenge` cookie（网页端点「统一认证登录」时就是这么做的）
//! 3. IAAA 登录（appID=anyshare，回调 `/oauth2/signin`）→ iaaa_token
//! 4. GET `/oauth2/signin?_rand=…&token=<iaaa_token>`：服务端凭 cookie 里的 challenge 通过 Hydra 的
//!    login/consent，重定向到 `/anyshare/oauth2/login/callback`，种下 `client.oauth2_token`、
//!    `client.oauth2_refresh_token`、`id_token` 三个 cookie
//!
//! 访问令牌 1 小时过期，由 [`crate::api`] 通过 `/anyshare/oauth2/login/refreshToken` 自动续期。

use crate::client::{self, DISK_BASE};
use anyhow::{anyhow, bail, Context, Result};
use colored::Colorize;
use pkuinfo_common::{
    credential,
    iaaa::{self, IaaaConfig},
    session::{Session, Store},
};
use reqwest_cookie_store::CookieStoreMutex;
use std::sync::Arc;

pub const APP_NAME: &str = "disk";

const IAAA_APP_ID: &str = "anyshare";
const IAAA_REDIRECT: &str = "https://disk.pku.edu.cn/oauth2/signin";
const MAX_HOPS: usize = 15;

/// 登录后必须拿到的 cookie
pub const ACCESS_COOKIE: &str = "client.oauth2_token";
const REFRESH_COOKIE: &str = "client.oauth2_refresh_token";

fn iaaa_config() -> IaaaConfig {
    IaaaConfig {
        app_id: IAAA_APP_ID.to_string(),
        redirect_url: IAAA_REDIRECT.to_string(),
    }
}

pub async fn login_with_password(username: Option<&str>) -> Result<()> {
    let store = Store::new(APP_NAME)?;
    let cred = credential::resolve_credential(username)?;
    let otp_code = pkuinfo_common::otp::get_current_otp(store.config_dir())?;
    if otp_code.is_some() {
        println!("{} 已自动填入手机令牌", "[otp]".cyan());
    }

    let (cookie_store, client, challenge) = start_oauth().await?;
    let iaaa_token = iaaa::login_password(
        &client::build_simple()?,
        &iaaa_config(),
        &cred.username,
        &cred.password,
        otp_code.as_deref(),
    )
    .await?;
    complete_login(
        &store,
        cookie_store,
        &client,
        &challenge,
        &iaaa_token.token,
        &cred.username,
    )
    .await
}

pub async fn login_with_qrcode(qr_mode: pkuinfo_common::qr::QrDisplayMode) -> Result<()> {
    let store = Store::new(APP_NAME)?;
    let (cookie_store, client, challenge) = start_oauth().await?;
    let iaaa_token = iaaa::login_qrcode(
        &client::build_simple()?,
        &iaaa_config(),
        store.config_dir(),
        qr_mode,
    )
    .await?;
    complete_login(
        &store,
        cookie_store,
        &client,
        &challenge,
        &iaaa_token.token,
        "",
    )
    .await
}

/// 逐跳跟随重定向（cookie 在每一跳都要落进同一个 jar），返回最终 URL 和状态码
async fn follow(client: &reqwest::Client, start: &str) -> Result<(url::Url, reqwest::StatusCode)> {
    let mut url = url::Url::parse(start)?;
    for _ in 0..MAX_HOPS {
        let resp = client
            .get(url.clone())
            .send()
            .await
            .with_context(|| format!("请求 {} 失败", url.path()))?;
        let status = resp.status();
        let next = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .filter(|_| status.is_redirection())
            .map(String::from);
        match next {
            Some(loc) => url = url.join(&loc)?,
            None => return Ok((url, status)),
        }
    }
    bail!("登录重定向次数过多")
}

/// 第 1 步：发起网页端 OAuth2 登录，拿到 Hydra 的 login_challenge
async fn start_oauth() -> Result<(Arc<CookieStoreMutex>, reqwest::Client, String)> {
    let cookie_store = Arc::new(CookieStoreMutex::default());
    let client = client::build_no_redirect(cookie_store.clone())?;
    let (url, _) = follow(
        &client,
        &format!("{DISK_BASE}/anyshare/oauth2/login?redirect=%2Fanyshare%2F"),
    )
    .await?;
    let challenge = url
        .query_pairs()
        .find(|(k, _)| k == "login_challenge")
        .map(|(_, v)| v.into_owned())
        .ok_or_else(|| anyhow!("网盘没有返回 login_challenge（停在 {}）", url.path()))?;
    Ok((cookie_store, client, challenge))
}

async fn complete_login(
    store: &Store,
    cookie_store: Arc<CookieStoreMutex>,
    client: &reqwest::Client,
    challenge: &str,
    iaaa_token: &str,
    username: &str,
) -> Result<()> {
    println!("{} 完成网盘登录...", "[+]".green());
    let base = url::Url::parse(DISK_BASE)?;
    cookie_store
        .lock()
        .map_err(|_| anyhow!("锁定 cookie store 失败"))?
        .parse(&format!("login_challenge={challenge}; Path=/"), &base)
        .context("写入 login_challenge cookie 失败")?;

    let rand = format!("0.{}", chrono::Utc::now().timestamp_subsec_nanos());
    let (url, status) = follow(
        client,
        &format!("{DISK_BASE}/oauth2/signin?_rand={rand}&token={iaaa_token}"),
    )
    .await?;

    let has = |name: &str| -> Result<bool> {
        Ok(cookie_store
            .lock()
            .map_err(|_| anyhow!("锁定 cookie store 失败"))?
            .iter_any()
            .any(|c| c.name() == name && !c.value().is_empty()))
    };
    if !has(ACCESS_COOKIE)? || !has(REFRESH_COOKIE)? {
        bail!(
            "网盘没有发放访问令牌（停在 {}，HTTP {}），请稍后重试",
            url.path(),
            status.as_u16()
        );
    }

    let mut session = Session::new(String::new());
    if !username.is_empty() {
        session.uid = Some(username.to_string());
    }
    store.save_session(&session)?;
    store.save_cookie_store(&cookie_store)?;

    println!();
    println!("{} 北大网盘登录成功！", "[done]".green().bold());
    if !username.is_empty() {
        println!("  学号     = {username}");
    }
    println!("  配置目录 = {}", store.config_dir().display());
    Ok(())
}

pub fn status() -> Result<()> {
    let store = Store::new(APP_NAME)?;
    match store.load_session()? {
        Some(s) => {
            println!("{} 已登录", "●".green());
            println!(
                "  创建时间 = {}",
                s.created_at.format("%Y-%m-%d %H:%M:%S UTC")
            );
            if let Some(uid) = &s.uid {
                println!("  学号     = {uid}");
            }
            println!("  配置目录 = {}", store.config_dir().display());
        }
        None => println!("{} 未登录。运行 `disk login -p` 开始。", "○".red()),
    }
    Ok(())
}

pub fn logout() -> Result<()> {
    Store::new(APP_NAME)?.clear()?;
    println!("{} 已清除本地会话", "✓".green());
    Ok(())
}
