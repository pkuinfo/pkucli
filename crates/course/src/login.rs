//! 教学网登录流程
//!
//! 完整流程：
//! 1. IAAA 登录（密码 or 扫码）→ 获得 iaaa_token
//! 2. 用 iaaa_token 回调 course.pku.edu.cn SSO → 建立 Blackboard 会话

use crate::client::{self, SSO_LOGIN};
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use pkuinfo_common::{
    credential,
    iaaa::{self, IaaaConfig},
    session::{Session, Store},
};

const APP_NAME: &str = "course";

/// IAAA OAuth redirect URL（与 pku3b 一致）
const OAUTH_REDIR: &str =
    "http://course.pku.edu.cn/webapps/bb-sso-BBLEARN/execute/authValidate/campusLogin";

fn iaaa_config() -> IaaaConfig {
    IaaaConfig {
        app_id: "blackboard".to_string(),
        redirect_url: OAUTH_REDIR.to_string(),
    }
}

/// 用户名密码登录
pub async fn login_with_password(username: Option<&str>) -> Result<()> {
    let store = Store::new(APP_NAME)?;
    check_existing_session(&store)?;

    let cred = credential::resolve_credential(username)?;

    let simple_client = client::build_simple()?;
    let config = iaaa_config();

    let otp_code = pkuinfo_common::otp::get_current_otp(store.config_dir())?;
    if otp_code.is_some() {
        println!("{} 已自动填入手机令牌", "[otp]".cyan());
    }
    let iaaa_token = iaaa::login_password(
        &simple_client,
        &config,
        &cred.username,
        &cred.password,
        otp_code.as_deref(),
    )
    .await?;

    complete_bb_login(&store, &iaaa_token.token).await
}

/// 扫码登录
pub async fn login_with_qrcode(qr_mode: pkuinfo_common::qr::QrDisplayMode) -> Result<()> {
    let store = Store::new(APP_NAME)?;
    check_existing_session(&store)?;

    let simple_client = client::build_simple()?;
    let config = iaaa_config();

    let iaaa_token =
        iaaa::login_qrcode(&simple_client, &config, store.config_dir(), qr_mode).await?;

    complete_bb_login(&store, &iaaa_token.token).await
}

/// IAAA 认证成功后，完成 Blackboard SSO 登录
async fn complete_bb_login(store: &Store, iaaa_token: &str) -> Result<()> {
    println!("{} 完成教学网登录...", "[+]".green());

    let cookie_store = store.load_cookie_store()?;
    let client = client::build(cookie_store.clone())?;

    // 用 IAAA token 访问 SSO 回调 URL，建立 Blackboard 会话
    let rand_val: f64 = rand::random();
    let sso_url = format!("{SSO_LOGIN}?_rand={rand_val:.20}&token={iaaa_token}");

    let resp = client
        .get(&sso_url)
        .send()
        .await
        .context("SSO 回调请求失败")?;

    if !resp.status().is_success() && !resp.status().is_redirection() {
        return Err(anyhow!("SSO 登录失败: HTTP {}", resp.status()));
    }
    // 消费 body，确保 cookies 被存储
    let _ = resp.bytes().await?;

    // 验证登录是否成功：访问主页
    let home_resp = client
        .get(format!(
            "{}/webapps/portal/execute/tabs/tabAction?tab_tab_group_id=_1_1",
            client::COURSE_BASE
        ))
        .send()
        .await
        .context("访问教学网主页失败")?;

    if !home_resp.status().is_success() {
        return Err(anyhow!(
            "教学网登录验证失败: HTTP {}",
            home_resp.status()
        ));
    }
    let _ = home_resp.bytes().await?;

    // 保存会话（cookie-based session，默认 24 小时过期）
    let mut session = Session::new(iaaa_token.to_string());
    session.expires_at = Some(chrono::Utc::now().timestamp() + 24 * 3600);
    store.save_session(&session)?;
    store.save_cookie_store(&cookie_store)?;

    println!();
    println!("{} 教学网登录成功！", "[done]".green().bold());
    println!("  配置目录 = {}", store.config_dir().display());
    Ok(())
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
                "  created_at = {}",
                s.created_at.format("%Y-%m-%d %H:%M:%S UTC")
            );
            println!("  config dir = {}", store.config_dir().display());
        }
        None => {
            println!(
                "{} 未登录。运行 `course login` 开始扫码登录，或 `course login -p` 密码登录。",
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
