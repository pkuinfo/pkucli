//! 选课网登录流程
//!
//! 完整流程：
//! 1. IAAA 登录（密码 or 扫码）→ 获得 iaaa_token
//! 2. 用 iaaa_token 回调 elective.pku.edu.cn SSO → 建立选课网会话
//!
//! 双学位用户需要选择 主修(bzx) / 辅双(bfx)

use crate::client::{self, OAUTH_REDIR, SSO_LOGIN};
use crate::config::ElectiveConfig;
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use info_common::{
    iaaa::{self, IaaaConfig},
    session::{Session, Store},
};
use reqwest_cookie_store::CookieStoreMutex;
use std::io::{self, Write};
use std::sync::Arc;

pub const APP_NAME: &str = "elective";

/// 双学位类型
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum DualDegree {
    /// 主修
    Major,
    /// 辅双
    Minor,
}

fn iaaa_config() -> IaaaConfig {
    IaaaConfig {
        app_id: "syllabus".to_string(),
        redirect_url: OAUTH_REDIR.to_string(),
    }
}

/// 用户名密码登录
pub async fn login_with_password(
    username: Option<&str>,
    dual: Option<&DualDegree>,
) -> Result<()> {
    let store = Store::new(APP_NAME)?;
    check_existing_session(&store)?;

    let mut cfg = ElectiveConfig::load(store.config_dir())?;

    let username = match username {
        Some(u) => u.to_string(),
        None => match &cfg.username {
            Some(u) => {
                println!("{} 使用已保存的用户名: {}", "[info]".cyan(), u);
                u.clone()
            }
            None => {
                print!("学号/职工号: ");
                io::stdout().flush()?;
                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                input.trim().to_string()
            }
        },
    };

    if username.is_empty() {
        return Err(anyhow!("用户名不能为空"));
    }

    // 保存用户名以便下次使用
    cfg.username = Some(username.clone());
    cfg.save(store.config_dir())?;

    print!("密码: ");
    io::stdout().flush()?;
    let password = rpassword::read_password().context("读取密码失败")?;
    if password.is_empty() {
        return Err(anyhow!("密码不能为空"));
    }

    let simple_client = client::build_simple()?;
    let config = iaaa_config();

    let iaaa_token =
{
        let otp_code = info_common::otp::get_current_otp(store.config_dir())?;
        if otp_code.is_some() {
            println!("{} 已自动填入手机令牌", "[otp]".cyan());
        }
        iaaa::login_password(
            &simple_client,
            &config,
            &username,
            &password,
            otp_code.as_deref(),
        )
        .await?
    };

    complete_sso_login(&store, &iaaa_token.token, dual, &username).await
}

/// 扫码登录
pub async fn login_with_qrcode(
    qr_mode: info_common::qr::QrDisplayMode,
    dual: Option<&DualDegree>,
) -> Result<()> {
    let store = Store::new(APP_NAME)?;
    check_existing_session(&store)?;

    let simple_client = client::build_simple()?;
    let config = iaaa_config();

    let iaaa_token =
        iaaa::login_qrcode(&simple_client, &config, store.config_dir(), qr_mode).await?;

    // 扫码登录不需要 username 做 SSO，但 dual degree 可能需要
    complete_sso_login(&store, &iaaa_token.token, dual, "").await
}

/// IAAA 认证成功后，完成选课网 SSO 登录
async fn complete_sso_login(
    store: &Store,
    iaaa_token: &str,
    dual: Option<&DualDegree>,
    username: &str,
) -> Result<()> {
    println!("{} 完成选课网登录...", "[+]".green());

    let cookie_store = store.load_cookie_store()?;
    let http = client::build(cookie_store.clone())?;

    let rand_val: f64 = rand::random();
    let sso_url = format!("{SSO_LOGIN}?_rand={rand_val:.20}&token={iaaa_token}");

    let resp = http
        .get(&sso_url)
        .send()
        .await
        .context("SSO 回调请求失败")?;

    let status = resp.status();

    if status.is_success() {
        // HTTP 200 — 可能是双学位选择页面
        let body = resp.text().await?;

        if let Some(dual_type) = dual {
            // 需要双学位选择
            let sida = extract_sida(&body)?;
            let sttp = match dual_type {
                DualDegree::Major => "bzx",
                DualDegree::Minor => "bfx",
            };

            let dual_url = format!("{SSO_LOGIN}?sida={sida}&sttp={sttp}");
            let dual_resp = http
                .get(&dual_url)
                .send()
                .await
                .context("双学位选择请求失败")?;

            follow_redirects(&http, &cookie_store, dual_resp).await?;
        } else if body.contains("div1") && body.contains("div2") {
            return Err(anyhow!(
                "检测到双学位账号。请使用 --dual major 或 --dual minor 指定"
            ));
        } else {
            // 正常 200，非双学位
            // 有些情况下 200 也代表登录成功
        }
    } else if status.is_redirection() {
        follow_redirects(&http, &cookie_store, resp).await?;
    } else {
        return Err(anyhow!("SSO 登录失败: HTTP {status}"));
    }

    // 保存会话
    let mut session = Session::new(iaaa_token.to_string());
    if !username.is_empty() {
        session.uid = Some(username.to_string());
    }
    store.save_session(&session)?;
    store.save_cookie_store(&cookie_store)?;

    println!();
    println!("{} 选课网登录成功！", "[done]".green().bold());
    println!("  配置目录 = {}", store.config_dir().display());
    Ok(())
}

/// 跟随 SSO 重定向链（http → https → 最终页面）
async fn follow_redirects(
    http: &reqwest::Client,
    _cookie_store: &Arc<CookieStoreMutex>,
    initial_resp: reqwest::Response,
) -> Result<()> {
    let mut resp = initial_resp;
    for _ in 0..5 {
        if !resp.status().is_redirection() {
            let _ = resp.bytes().await?;
            return Ok(());
        }
        let location = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| anyhow!("重定向缺少 Location 头"))?
            .to_string();

        let _ = resp.bytes().await?;

        resp = http
            .get(&location)
            .send()
            .await
            .with_context(|| format!("重定向请求失败: {location}"))?;
    }
    let _ = resp.bytes().await?;
    Ok(())
}

/// 从双学位选择页面提取 sida 参数
fn extract_sida(body: &str) -> Result<String> {
    let re = regex::Regex::new(r"\?sida=(\S{32})&sttp=")
        .context("sida 正则编译失败")?;
    let caps = re
        .captures(body)
        .ok_or_else(|| anyhow!("无法从页面中提取 sida 参数"))?;
    Ok(caps[1].to_string())
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
    let cfg = ElectiveConfig::load(store.config_dir())?;

    match store.load_session()? {
        Some(s) => {
            println!("{} 已登录", "●".green());
            println!(
                "  创建时间 = {}",
                s.created_at.format("%Y-%m-%d %H:%M:%S UTC")
            );
            if let Some(uid) = &s.uid {
                println!("  用户名   = {}", uid);
            }
            println!("  验证码   = {}", cfg.captcha);
            println!("  自动选课 = {} 门课程", cfg.auto_elect.len());
            println!("  配置目录 = {}", store.config_dir().display());
        }
        None => {
            println!(
                "{} 未登录。运行 `elective login` 开始。",
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
