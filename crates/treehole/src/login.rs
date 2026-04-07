//! 树洞登录流程
//!
//! 完整流程：
//! 1. IAAA 登录（密码 or 扫码）→ 获得 iaaa_token
//! 2. 用 iaaa_token 回调 treehole 的 cas_iaaa_login → 获得 JWT + cookies
//! 3. （可选）短信验证

use crate::client::{self, TREEHOLE_BASE};
use crate::verify;
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use info_common::{
    iaaa::{self, IaaaConfig},
    session::{Session, Store},
};
use std::io::{self, Write};

const APP_NAME: &str = "treehole";

fn iaaa_config(device_uuid: &str) -> IaaaConfig {
    IaaaConfig {
        app_id: "PKU Helper".to_string(),
        redirect_url: format!(
            "{TREEHOLE_BASE}/chapi/cas_iaaa_login?version=3&uuid={device_uuid}&plat=web"
        ),
    }
}

/// 获取或生成设备 UUID
fn get_device_uuid(store: &Store) -> String {
    // 尝试从已有 session 中读取
    if let Ok(Some(sess)) = store.load_session() {
        if let Some(uuid) = sess.extra.get("device_uuid").and_then(|v| v.as_str()) {
            return uuid.to_string();
        }
    }
    // 生成新的
    let uuid = uuid::Uuid::new_v4();
    // 取最后 12 个字符，仿照 Web 端格式
    let hex = uuid.simple().to_string();
    hex[20..].to_string()
}

fn full_device_uuid(short_uuid: &str) -> String {
    let full = uuid::Uuid::new_v4().to_string();
    format!(
        "Web_PKUHOLE_2.0.0_WEB_UUID_{}-{}",
        &full[..23],
        short_uuid
    )
}

/// 用户名密码登录
pub async fn login_with_password(username: Option<&str>) -> Result<()> {
    let store = Store::new(APP_NAME)?;
    check_existing_session(&store)?;

    let username = match username {
        Some(u) => u.to_string(),
        None => {
            print!("学号/职工号: ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            input.trim().to_string()
        }
    };

    if username.is_empty() {
        return Err(anyhow!("用户名不能为空"));
    }

    print!("密码: ");
    io::stdout().flush()?;
    let password = rpassword::read_password().context("读取密码失败")?;
    if password.is_empty() {
        return Err(anyhow!("密码不能为空"));
    }

    let simple_client = client::build_simple()?;
    let device_uuid = get_device_uuid(&store);
    let config = iaaa_config(&device_uuid);

    let otp_code = info_common::otp::get_current_otp(store.config_dir())?;
    if otp_code.is_some() {
        println!("{} 已自动填入手机令牌", "[otp]".cyan());
    }
    let iaaa_token = iaaa::login_password(
        &simple_client,
        &config,
        &username,
        &password,
        otp_code.as_deref(),
    )
    .await?;

    complete_treehole_login(&store, &iaaa_token.token, &device_uuid).await
}

/// 扫码登录
pub async fn login_with_qrcode(qr_mode: info_common::qr::QrDisplayMode) -> Result<()> {
    let store = Store::new(APP_NAME)?;
    check_existing_session(&store)?;

    let simple_client = client::build_simple()?;
    let device_uuid = get_device_uuid(&store);
    let config = iaaa_config(&device_uuid);

    let iaaa_token =
        iaaa::login_qrcode(&simple_client, &config, store.config_dir(), qr_mode).await?;

    complete_treehole_login(&store, &iaaa_token.token, &device_uuid).await
}

/// IAAA 认证成功后，完成树洞的登录回调
async fn complete_treehole_login(store: &Store, iaaa_token: &str, device_uuid: &str) -> Result<()> {
    println!("{} 完成树洞登录...", "[+]".green());

    let cookie_store = store.load_cookie_store()?;
    let client = client::build(cookie_store.clone())?;

    // 构造回调 URL
    let callback_url = format!(
        "{TREEHOLE_BASE}/chapi/cas_iaaa_login?version=3&uuid={device_uuid}&plat=web&_rand={}&token={iaaa_token}",
        rand::random::<f64>()
    );

    // 发送回调请求 — 服务器会 302 重定向到 iaaa_success 页面
    let resp = client
        .get(&callback_url)
        .send()
        .await
        .context("树洞回调请求失败")?;

    // 解析重定向 URL 或直接从响应中提取 token
    let (jwt_token, expires_in, uid) = if resp.status().is_redirection() {
        let location = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| anyhow!("重定向缺少 location"))?
            .to_string();
        parse_iaaa_success_url(&location)?
    } else {
        // 可能直接返回了 200，尝试从 set-cookie 中提取
        // 先消费 body
        let _ = resp.bytes().await?;
        // 从 cookie store 中提取
        extract_from_cookies(&cookie_store)?
    };

    // 检查是否需要短信验证（树洞特有，首次/定期）
    println!("{} 检查认证状态...", "[+]".green());
    let full_uuid = full_device_uuid(device_uuid);

    // 先访问 version 接口刷新 cookies
    let version_resp = client
        .get(format!("{TREEHOLE_BASE}/chapi/version?t={}", chrono::Utc::now().timestamp_millis()))
        .header("authorization", format!("Bearer {jwt_token}"))
        .header("uuid", &full_uuid)
        .send()
        .await;
    if let Ok(resp) = version_resp {
        let _ = resp.bytes().await;
    }

    verify::check_and_verify(&client, &jwt_token, &full_uuid).await?;

    // 保存会话
    let mut session = Session::new(jwt_token.clone());
    session.expires_at = Some(expires_in);
    session.uid = Some(uid.clone());
    session.extra = serde_json::json!({
        "device_uuid": device_uuid,
        "full_uuid": full_uuid,
    });
    store.save_session(&session)?;
    store.save_cookie_store(&cookie_store)?;

    println!();
    println!("{} 登录成功！", "[done]".green().bold());
    println!("  uid       = {}", uid.bold());
    println!("  token     = {}...", &jwt_token[..40.min(jwt_token.len())]);
    println!("  expires   = {}", format_timestamp(expires_in));
    println!("  配置目录   = {}", store.config_dir().display());
    Ok(())
}

fn parse_iaaa_success_url(url: &str) -> Result<(String, i64, String)> {
    let parsed = if url.starts_with("http") {
        url::Url::parse(url)
    } else {
        url::Url::parse(&format!("{TREEHOLE_BASE}{url}"))
    }
    .context("解析回调 URL 失败")?;

    let mut token = None;
    let mut expires_in = 0i64;
    let mut uid = String::new();

    for (k, v) in parsed.query_pairs() {
        match k.as_ref() {
            "token" => token = Some(v.to_string()),
            "expires_in" => expires_in = v.parse().unwrap_or(0),
            "uid" => uid = v.to_string(),
            _ => {}
        }
    }

    let token = token.ok_or_else(|| anyhow!("回调 URL 中缺少 token"))?;
    Ok((token, expires_in, uid))
}

fn extract_from_cookies(
    cookie_store: &std::sync::Arc<reqwest_cookie_store::CookieStoreMutex>,
) -> Result<(String, i64, String)> {
    let guard = cookie_store
        .lock()
        .map_err(|e| anyhow!("锁定 cookie store 失败: {e}"))?;

    let mut token = None;
    let mut expires_in = 0i64;
    let mut uid = String::new();

    for c in guard.iter_any() {
        match c.name() {
            "pku_token" => token = Some(c.value().to_string()),
            "pku_expires_in" => expires_in = c.value().parse().unwrap_or(0),
            "pku_uid" => uid = c.value().to_string(),
            _ => {}
        }
    }

    let token = token.ok_or_else(|| anyhow!("cookie 中缺少 pku_token"))?;
    Ok((token, expires_in, uid))
}

fn check_existing_session(store: &Store) -> Result<()> {
    if let Some(old) = store.load_session()? {
        if !old.is_expired() {
            println!(
                "{} 检测到已有登录会话 (uid={})，继续将覆盖。",
                "[info]".cyan(),
                old.uid.as_deref().unwrap_or("?")
            );
        }
    }
    Ok(())
}

fn format_timestamp(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| ts.to_string())
}

/// 查看当前登录状态
pub fn status() -> Result<()> {
    let store = Store::new(APP_NAME)?;
    match store.load_session()? {
        Some(s) => {
            let expired = s.is_expired();
            if expired {
                println!("{} 会话已过期", "●".red());
            } else {
                println!("{} 已登录", "●".green());
            }
            if let Some(uid) = &s.uid {
                println!("  uid        = {uid}");
            }
            println!(
                "  token      = {}...",
                &s.token[..40.min(s.token.len())]
            );
            if let Some(exp) = s.expires_at {
                println!("  expires_at = {}", format_timestamp(exp));
            }
            println!("  created_at = {}", s.created_at.format("%Y-%m-%d %H:%M:%S UTC"));
            println!("  config dir = {}", store.config_dir().display());
        }
        None => {
            println!(
                "{} 未登录。运行 `treehole login` 开始扫码登录，或 `treehole login -p` 密码登录。",
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
