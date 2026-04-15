//! 财务综合信息门户登录流程
//!
//! 完整流程：
//! 1. IAAA 登录（app_id=IIPF）→ 获得 iaaa_token
//! 2. GET `/WFManager/home2.jsp?token=...` → 建立 JSESSIONID 等跨子系统 cookie
//! 3. GET `/WF_CWBS/main.jsp` → 建立 WF_CWBS 专属的 `WF_CWBSsid` / `WF_CWBSroles` cookie

use crate::client::{self, CWFW_BASE};
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use pkuinfo_common::{
    credential,
    iaaa::{self, IaaaConfig},
    session::{Session, Store},
};

pub const APP_NAME: &str = "cwfw";

const IAAA_APP_ID: &str = "IIPF";
const IAAA_REDIRECT: &str = "https://cwfw.pku.edu.cn/WFManager/home2.jsp";

fn iaaa_config() -> IaaaConfig {
    IaaaConfig {
        app_id: IAAA_APP_ID.to_string(),
        redirect_url: IAAA_REDIRECT.to_string(),
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

    complete_login(&store, &iaaa_token.token, &cred.username).await
}

/// 扫码登录
pub async fn login_with_qrcode(qr_mode: pkuinfo_common::qr::QrDisplayMode) -> Result<()> {
    let store = Store::new(APP_NAME)?;
    check_existing_session(&store)?;

    let simple_client = client::build_simple()?;
    let config = iaaa_config();

    let iaaa_token =
        iaaa::login_qrcode(&simple_client, &config, store.config_dir(), qr_mode).await?;

    complete_login(&store, &iaaa_token.token, "").await
}

/// IAAA 认证成功后，完成 cwfw 登录
async fn complete_login(store: &Store, iaaa_token: &str, username: &str) -> Result<()> {
    println!("{} 完成财务门户登录...", "[+]".green());

    let cookie_store = store.load_cookie_store()?;
    let client = client::build(cookie_store.clone())?;

    // Step 1: IAAA token → home2.jsp 建立跨子系统 session
    //         home2.jsp 会跳转到 home3.jsp（门户主页）。
    let rand_val: f64 = rand::random();
    let home_url = format!("{IAAA_REDIRECT}?_rand={rand_val:.20}&token={iaaa_token}");
    let resp = client
        .get(&home_url)
        .send()
        .await
        .context("访问 cwfw home2.jsp 失败")?;
    let status = resp.status();
    let final_url = resp.url().clone();
    let body = resp.text().await?;
    if !status.is_success() && !status.is_redirection() {
        return Err(anyhow!(
            "home2.jsp 登录失败: HTTP {status}, URL={final_url}"
        ));
    }
    if body.contains("iaaa.pku.edu.cn") && body.contains("oauth.jsp") {
        return Err(anyhow!("cwfw 拒绝 IAAA token，请检查账号状态"));
    }
    // home2.jsp 的返回是 HTML 里嵌了一段 `$.ajax` 的 JS，这段 JS 去调用
    // `findpages_postData.action?token=<iaaa_token>`。这个 POST 才是真正给服务端做二次
    // 鉴权 + 初始化所有 `WF_*` 子系统 session 的地方，所以我们在 Rust 端必须手动模拟。
    let ajax_resp = client
        .post(format!("{CWFW_BASE}/WFManager/findpages_postData.action"))
        .header("x-requested-with", "XMLHttpRequest")
        .header(
            "content-type",
            "application/x-www-form-urlencoded; charset=UTF-8",
        )
        .form(&[
            ("token", iaaa_token),
            (
                "timeStamp",
                &chrono::Utc::now().timestamp_millis().to_string(),
            ),
        ])
        .send()
        .await
        .context("findpages_postData.action 请求失败")?;
    let ajax_status = ajax_resp.status();
    let ajax_text = ajax_resp.text().await?;
    if !ajax_status.is_success() {
        return Err(anyhow!(
            "findpages_postData.action HTTP {ajax_status}: {ajax_text}"
        ));
    }
    if ajax_text.trim() != "ok" {
        return Err(anyhow!(
            "findpages_postData.action 未返回 ok: '{ajax_text}'，账号可能已停用"
        ));
    }

    // Step 1.5: GET home3.jsp — 主面板 HTML 里每个子系统的 `<div id="WF_*">` 都带着
    // `url="WINOPEN../WF_CWBS/main2.jsp?context=...&token=<subsystem_token>&pId=WF_CWBS"`，
    // 我们需要从里面抠出 WF_CWBS 的真实入口 URL。
    let home3_resp = client
        .get(format!("{CWFW_BASE}/WFManager/home3.jsp"))
        .send()
        .await
        .context("访问 home3.jsp 失败")?;
    let home3_status = home3_resp.status();
    let home3_body = home3_resp.text().await?;
    if !home3_status.is_success() {
        return Err(anyhow!("home3.jsp HTTP {home3_status}"));
    }
    let entry_url = extract_wf_cwbs_entry(&home3_body)
        .ok_or_else(|| anyhow!("home3.jsp 响应中未找到 WF_CWBS 子系统入口"))?;

    // Step 2: 访问 WF_CWBS 子系统入口（main2.jsp + 子系统 token），完成子系统鉴权。
    // entry_url 是相对 `/WFManager/` 的 `../WF_CWBS/main2.jsp?...`，拼接成绝对 URL。
    let main_url = resolve_relative(&format!("{CWFW_BASE}/WFManager/home3.jsp"), &entry_url)?;
    let resp = client
        .get(&main_url)
        .header("referer", format!("{CWFW_BASE}/WFManager/home3.jsp"))
        .send()
        .await
        .context("访问 WF_CWBS 子系统入口失败")?;
    let status = resp.status();
    let final_url = resp.url().clone();
    let body = resp.text().await?;
    if !status.is_success() {
        return Err(anyhow!(
            "WF_CWBS 子系统入口访问失败: HTTP {status}, URL={final_url}"
        ));
    }
    if body.contains("login.html") && body.contains("系统超时") {
        return Err(anyhow!("会话未能建立，请重新登录"));
    }

    // 保存会话（cookie-based，默认 24 小时过期）
    let mut session = Session::new(iaaa_token.to_string());
    session.expires_at = Some(chrono::Utc::now().timestamp() + 24 * 3600);
    if !username.is_empty() {
        session.uid = Some(username.to_string());
    }
    store.save_session(&session)?;
    store.save_cookie_store(&cookie_store)?;

    println!();
    println!("{} 财务门户登录成功！", "[done]".green().bold());
    println!("  配置目录 = {}", store.config_dir().display());
    Ok(())
}

fn check_existing_session(store: &Store) -> Result<()> {
    if let Some(old) = store.load_session()? {
        if !old.is_expired() {
            println!("{} 检测到已有登录会话，继续将覆盖。", "[info]".cyan());
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
            println!("{} 未登录。运行 `cwfw login` 开始。", "○".red());
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

/// 从 home3.jsp HTML 里抠出 `<div id="WF_CWBS" url="...">` 的 `url` 属性。
///
/// 属性值形如 `WINOPEN../WF_CWBS/main2.jsp?context=...&token=...&pId=WF_CWBS`。
/// 这里去掉前缀 `WINOPEN`，返回相对 URL。
fn extract_wf_cwbs_entry(html: &str) -> Option<String> {
    let re =
        regex::Regex::new(r#"id="WF_CWBS"\s+url="([^"]+)""#).ok()?;
    let caps = re.captures(html)?;
    let raw = caps.get(1)?.as_str();
    // 去掉 HTML 实体转义
    let raw = raw.replace("&amp;", "&");
    // 去掉 WINOPEN 前缀（客户端约定）
    let url = raw.strip_prefix("WINOPEN").unwrap_or(&raw);
    Some(url.to_string())
}

/// 把相对 URL 拼接成绝对 URL。
fn resolve_relative(base: &str, relative: &str) -> Result<String> {
    let base_url = reqwest::Url::parse(base).context("解析 base URL 失败")?;
    let joined = base_url
        .join(relative)
        .context("拼接相对 URL 失败")?;
    Ok(joined.to_string())
}

/// 加载已保存的会话（包含用户名）
pub fn load_session() -> Result<Session> {
    let store = Store::new(APP_NAME)?;
    let session = store
        .load_session()?
        .ok_or_else(|| anyhow!("未登录。请先运行 `cwfw login`"))?;
    if session.is_expired() {
        return Err(anyhow!("会话已过期。请重新运行 `cwfw login`"));
    }
    Ok(session)
}
