//! 校园卡登录流程
//!
//! 完整流程：
//! 1. IAAA 登录（密码 or 扫码）→ 获得 iaaa_token
//! 2. 用 iaaa_token 回调 portal SSO → 建立门户会话
//! 3. 门户 redirectToCard.do → 获得校园卡 token
//! 4. berserker-auth/cas/login/pku → 获得 JWT (synjones-auth)

use crate::client::{self, PORTAL_BASE};
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use info_common::{
    credential,
    iaaa::{self, IaaaConfig},
    session::{Session, Store},
};

pub const APP_NAME: &str = "campuscard";

const PORTAL_APP_ID: &str = "portal2017";
const PORTAL_REDIRECT: &str =
    "https://portal.pku.edu.cn/portal2017/ssoLogin.do";

fn iaaa_config() -> IaaaConfig {
    IaaaConfig {
        app_id: PORTAL_APP_ID.to_string(),
        redirect_url: PORTAL_REDIRECT.to_string(),
    }
}

/// 用户名密码登录
pub async fn login_with_password(username: Option<&str>) -> Result<()> {
    let store = Store::new(APP_NAME)?;
    check_existing_session(&store)?;

    let cred = credential::resolve_credential(username)?;

    let simple_client = client::build_simple()?;
    let config = iaaa_config();

    let iaaa_token = {
        let otp_code = info_common::otp::get_current_otp(store.config_dir())?;
        if otp_code.is_some() {
            println!("{} 已自动填入手机令牌", "[otp]".cyan());
        }
        iaaa::login_password(
            &simple_client,
            &config,
            &cred.username,
            &cred.password,
            otp_code.as_deref(),
        )
        .await?
    };

    complete_login(&store, &iaaa_token.token, &cred.username).await
}

/// 扫码登录
pub async fn login_with_qrcode(qr_mode: info_common::qr::QrDisplayMode) -> Result<()> {
    let store = Store::new(APP_NAME)?;
    check_existing_session(&store)?;

    let simple_client = client::build_simple()?;
    let config = iaaa_config();

    let iaaa_token =
        iaaa::login_qrcode(&simple_client, &config, store.config_dir(), qr_mode).await?;

    complete_login(&store, &iaaa_token.token, "").await
}

/// IAAA 认证成功后，完成校园卡登录
///
/// 需要手动控制重定向链来提取中间的 JWT：
/// ssoLogin.do → 302(设置 SESSION) →
/// redirectToCard.do → 302 → berserker-auth → 302(带 JWT) → ...
async fn complete_login(
    store: &Store,
    iaaa_token: &str,
    username: &str,
) -> Result<()> {
    // 构建一个不跟随重定向的客户端（共享 simple_client 的 cookie jar）
    // simple_client 已有内置 cookie jar，但我们需要手动处理重定向
    // 所以单独构建一个 no-redirect 客户端来执行门户→校园卡的流程
    let no_redirect = client::build_no_redirect()?;

    // Step 1: 用 IAAA token 登录门户（获取 SESSION cookie）
    println!("{} 登录门户...", "[1/3]".green());
    let rand_val: f64 = rand::random();
    let sso_url = format!(
        "{PORTAL_REDIRECT}?_rand={rand_val:.20}&token={iaaa_token}"
    );

    let resp = no_redirect
        .get(&sso_url)
        .send()
        .await
        .context("门户 SSO 请求失败")?;

    let status = resp.status();
    if !status.is_success() && !status.is_redirection() {
        return Err(anyhow!("门户 SSO 登录失败: HTTP {status}"));
    }
    // 消费 body（让 cookie jar 保存 SESSION cookie）
    let _ = resp.bytes().await?;

    // Step 2: 调用 redirectToCard.do（带 SESSION cookie）
    // 它会 302 → berserker-auth/cas/login/pku?token=xxx&targetUrl=...
    println!("{} 获取校园卡授权...", "[2/3]".green());
    let redirect_url = format!("{PORTAL_BASE}/util/redirectToCard.do");
    let resp = no_redirect
        .get(&redirect_url)
        .send()
        .await
        .context("redirectToCard 请求失败")?;

    let location = get_location(&resp, "redirectToCard")?;
    let _ = resp.bytes().await?;

    // Step 3: 跟随到 berserker-auth/cas/login/pku
    // 它会验证 token 并 302 → redirect URL 带 synjones-auth=<JWT>
    println!("{} 获取登录凭证...", "[3/3]".green());
    let resp = no_redirect
        .get(&location)
        .send()
        .await
        .context("berserker-auth 请求失败")?;

    let auth_location = get_location(&resp, "berserker-auth")?;
    let _ = resp.bytes().await?;

    // auth_location 包含 synjones-auth=<JWT>
    let jwt = extract_jwt(&auth_location)?;

    // 保存会话（JWT 默认 24 小时过期）
    let mut session = Session::new(jwt.clone());
    session.expires_at = Some(chrono::Utc::now().timestamp() + 24 * 3600);
    if !username.is_empty() {
        session.uid = Some(username.to_string());
    }
    store.save_session(&session)?;

    println!();
    println!("{} 校园卡登录成功！", "[done]".green().bold());
    println!("  配置目录 = {}", store.config_dir().display());
    Ok(())
}

/// 从响应中提取 Location 头
fn get_location(resp: &reqwest::Response, step: &str) -> Result<String> {
    resp.headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("{step} 未返回重定向（HTTP {}）", resp.status()))
}

/// 从重定向 URL 中提取 synjones-auth JWT
fn extract_jwt(url: &str) -> Result<String> {
    let parsed = reqwest::Url::parse(url)
        .context("解析重定向 URL 失败")?;

    for (k, v) in parsed.query_pairs() {
        if k == "synjones-auth" {
            return Ok(v.to_string());
        }
    }

    Err(anyhow!("重定向 URL 中未找到 synjones-auth 参数"))
}

fn check_existing_session(store: &Store) -> Result<()> {
    if let Some(old) = store.load_session()? {
        if !old.is_expired() {
            println!(
                "{} 检测到已有登录会话，继续将覆盖。",
                "[info]".cyan(),
            );
        }
    }
    Ok(())
}

/// 查看当前登录状态
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
                println!("  用户名   = {uid}");
            }
            println!("  配置目录 = {}", store.config_dir().display());
        }
        None => {
            println!(
                "{} 未登录。运行 `campuscard login` 开始。",
                "○".red()
            );
        }
    }
    Ok(())
}

/// 退出登录
pub fn logout() -> Result<()> {
    let store = Store::new(APP_NAME)?;
    store.clear()?;
    println!("{} 已清除本地会话", "✓".green());
    Ok(())
}

/// 加载已保存的 JWT
pub fn load_jwt() -> Result<String> {
    let store = Store::new(APP_NAME)?;
    let session = store
        .load_session()?
        .ok_or_else(|| anyhow!("未登录。请先运行 `campuscard login`"))?;
    if session.is_expired() {
        return Err(anyhow!("会话已过期。请重新运行 `campuscard login`"));
    }
    Ok(session.token)
}
