//! 学生邮箱登录（网易企业邮，不走 IAAA）
//!
//! 详见 `docs/stu-mail-api.md` 第 1 节：
//! 1. `GET prelogin.jsp?uid=<学号>@stu.pku.edu.cn` → 一次性 RSA 公钥 + `rand`
//! 2. `POST domainEntLogin`，密码 = RSA_PKCS1_v1_5(`密码#rand`) 的十六进制，`passtype=3`
//! 3. 302 `Location`：短信页 → 第 4 步；带 `sid` → 登录成功；其他 → 失败
//! 4. `send_code.jsp` 发短信，`POST mobileLogin?<短信页 query>`（`pass_2fa=1` 信任本设备）
//!
//! `_deviceId`（客户端生成，32 位十六进制）和服务器下发的 `_sDeviceId` cookie 长期保存，
//! 这样短信验证每台机器只需要一次。短信可以分两步完成（适合 AI Agent）：
//! `mail login -p` 发出验证码后退出，`mail login -p --code 123456` 用保存的状态完成登录。

use crate::client;
use crate::parse::{self, classify_location, LoginStep, SmsInfo};
use anyhow::{anyhow, bail, Context, Result};
use colored::Colorize;
use pkuinfo_common::{
    credential,
    session::{Session, Store},
};
use rand::Rng;
use reqwest_cookie_store::CookieStoreMutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;

pub const APP_NAME: &str = "mail";
pub const DOMAIN: &str = "stu.pku.edu.cn";

const ENTRY: &str = "https://mailh.qiye.163.com";
const ENTRY_XC: &str = "https://mailxc.qiye.163.com";
const COOKIE_URL: &str = "https://mailh.qiye.163.com/";
const MAX_HOPS: usize = 10;
/// 会话大约一天
const SESSION_TTL: i64 = 86400;
/// 待验证的短信登录保留 10 分钟
const PENDING_TTL: i64 = 600;
/// 退出登录时保留的设备 cookie（服务器据此免短信）
const DEVICE_COOKIES: [&str; 2] = ["_deviceId", "_sDeviceId"];

/// 已登录的会话
#[derive(Debug, Clone)]
pub struct MailSession {
    pub sid: String,
    /// 邮件服务器，如 mailshz.qiye.163.com
    pub host: String,
    /// 规范邮箱地址（发信时的 `attrs.account`）
    pub account: String,
}

pub fn load_session(store: &Store) -> Result<Option<MailSession>> {
    let Some(s) = store.load_session()? else {
        return Ok(None);
    };
    let host = s.extra.get("host").and_then(Value::as_str).unwrap_or("");
    if s.token.is_empty() || host.is_empty() {
        return Ok(None);
    }
    Ok(Some(MailSession {
        sid: s.token.clone(),
        host: host.to_string(),
        account: s.uid.clone().unwrap_or_default(),
    }))
}

// ─── 本机设备与待验证状态 ──────────────────────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
struct Device {
    /// 本机设备号，生成一次后不变
    device_id: String,
    /// 上次登录用的学号
    #[serde(default)]
    username: String,
    /// 该学号对应的规范邮箱地址
    #[serde(default)]
    account: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Pending {
    entry: String,
    username: String,
    query: String,
    account_name: String,
    uid: String,
    mobile: String,
    created_at: i64,
}

fn device_path(store: &Store) -> PathBuf {
    store.config_dir().join("device.json")
}

fn pending_path(store: &Store) -> PathBuf {
    store.config_dir().join("pending.json")
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &PathBuf) -> Result<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).with_context(|| format!("读取 {} 失败", path.display()))?;
    Ok(serde_json::from_slice(&bytes).ok())
}

fn write_json<T: Serialize>(path: &PathBuf, value: &T) -> Result<()> {
    std::fs::write(path, serde_json::to_vec_pretty(value)?)
        .with_context(|| format!("写入 {} 失败", path.display()))
}

fn load_device(store: &Store) -> Result<Device> {
    if let Some(d) = read_json::<Device>(&device_path(store))? {
        if d.device_id.len() == 32 {
            return Ok(d);
        }
    }
    let mut rng = rand::thread_rng();
    let device_id: String = (0..32)
        .map(|_| char::from_digit(rng.gen_range(0..16), 16).unwrap_or('0'))
        .collect();
    let d = Device {
        device_id,
        ..Default::default()
    };
    write_json(&device_path(store), &d)?;
    Ok(d)
}

fn cookie_value(cookies: &CookieStoreMutex, name: &str) -> Result<Option<String>> {
    Ok(cookies
        .lock()
        .map_err(|_| anyhow!("锁定 cookie store 失败"))?
        .iter_any()
        .find(|c| c.name() == name && !c.value().is_empty())
        .map(|c| c.value().to_string()))
}

/// 网页端会把 `_deviceId` 写进 `.qiye.163.com` 的 cookie，这里照做
fn ensure_device_cookie(cookies: &CookieStoreMutex, device_id: &str) -> Result<()> {
    if cookie_value(cookies, "_deviceId")?.is_some() {
        return Ok(());
    }
    let url = url::Url::parse(COOKIE_URL)?;
    cookies
        .lock()
        .map_err(|_| anyhow!("锁定 cookie store 失败"))?
        .parse(
            &format!("_deviceId={device_id}; Domain=qiye.163.com; Path=/; Max-Age=31536000"),
            &url,
        )
        .context("写入 _deviceId cookie 失败")?;
    Ok(())
}

/// 学号可以带不带 `@stu.pku.edu.cn`
fn bare_id(username: &str) -> &str {
    username.split('@').next().unwrap_or(username).trim()
}

fn full_address(name: &str) -> String {
    if name.contains('@') {
        name.to_string()
    } else {
        format!("{name}@{DOMAIN}")
    }
}

// ─── 登录步骤 ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct PreLogin {
    rand: String,
    pubid: String,
    modulus: String,
    exponent: String,
    #[serde(default)]
    locked: bool,
    #[serde(default)]
    verify_code: bool,
    #[serde(default)]
    xc: bool,
}

/// 去掉可能的 JSONP 包裹
fn strip_jsonp(body: &str) -> &str {
    let b = body.trim();
    match (b.find('('), b.rfind(')')) {
        (Some(l), Some(r)) if !b.starts_with('{') && l < r => &b[l + 1..r],
        _ => b,
    }
}

async fn prelogin(client: &reqwest::Client, id: &str) -> Result<PreLogin> {
    let body = client
        .get(format!("{ENTRY}/login/prelogin.jsp"))
        .query(&[("uid", full_address(id))])
        .send()
        .await
        .context("获取登录公钥失败")?
        .text()
        .await
        .context("读取登录公钥失败")?;
    let v: Value = serde_json::from_str(strip_jsonp(&body))
        .with_context(|| format!("登录公钥格式异常：{}", parse::snippet(&body)))?;
    if v.get("code").and_then(Value::as_i64) != Some(200) {
        bail!("获取登录公钥失败：{}", parse::snippet(&body));
    }
    let pre: PreLogin = serde_json::from_value(v["data"].clone()).context("登录公钥缺少字段")?;
    if pre.locked {
        bail!("邮箱帐号已被锁定，请稍后再试或在网页端解锁");
    }
    if pre.verify_code {
        bail!("邮箱要求输入图片验证码（多次输错密码后出现），请先在网页端 mail.stu.pku.edu.cn 登录一次");
    }
    Ok(pre)
}

/// 逐跳跟随登录跳转，直到短信页 / sid / 失败
async fn settle(client: &reqwest::Client, mut resp: reqwest::Response) -> Result<LoginStep> {
    for _ in 0..MAX_HOPS {
        let status = resp.status();
        let url = resp.url().clone();
        let location = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .filter(|_| status.is_redirection())
            .map(String::from);
        let Some(location) = location else {
            let body = resp.text().await.unwrap_or_default();
            return Ok(LoginStep::Failed(format!(
                "登录没有跳转（HTTP {}，停在 {}）：{}",
                status.as_u16(),
                url.path(),
                parse::snippet(&body)
            )));
        };
        match classify_location(&url, &location)? {
            LoginStep::Next(next) => {
                resp = client
                    .get(next.clone())
                    .send()
                    .await
                    .with_context(|| format!("请求 {} 失败", next.path()))?;
            }
            other => return Ok(other),
        }
    }
    bail!("登录重定向次数过多")
}

/// 第 1–3 步：用密码登录，返回登录结果（成功 / 需要短信 / 失败）
async fn password_step(
    store: &Store,
    cookies: &Arc<CookieStoreMutex>,
    client: &reqwest::Client,
    username: &str,
    password: &str,
) -> Result<(String, LoginStep)> {
    let id = bare_id(username);
    let device = load_device(store)?;
    ensure_device_cookie(cookies, &device.device_id)?;

    let pre = prelogin(client, id).await?;
    let entry = if pre.xc { ENTRY_XC } else { ENTRY };
    let encrypted = parse::encrypt_password(&pre.modulus, &pre.exponent, password, &pre.rand)?;
    let s_device_id = cookie_value(cookies, "_sDeviceId")?.unwrap_or_default();

    let form = [
        ("domain", DOMAIN),
        ("account_name", id),
        ("password", encrypted.as_str()),
        ("pubid", pre.pubid.as_str()),
        ("passtype", "3"),
        ("support_verify_code", "1"),
        ("verify_code", ""),
        ("secure", "1"),
        ("all_secure", "1"),
        ("ch", ""),
        ("_deviceId", device.device_id.as_str()),
        ("_sDeviceId", s_device_id.as_str()),
    ];
    let resp = client
        .post(format!("{entry}/login/domainEntLogin"))
        .form(&form)
        .send()
        .await
        .context("提交邮箱登录失败")?;
    Ok((entry.to_string(), settle(client, resp).await?))
}

/// 发送短信验证码，记下第二步要用的状态
async fn send_sms(
    store: &Store,
    cookies: &Arc<CookieStoreMutex>,
    client: &reqwest::Client,
    entry: &str,
    username: &str,
    info: &SmsInfo,
) -> Result<()> {
    let account_name = if info.account_name.is_empty() {
        bare_id(username).to_string()
    } else {
        info.account_name.clone()
    };
    // 网页端会先查一次 MFA 信息；结果只是提示用，失败不影响
    let _ = client
        .get(format!("{entry}/qiyeurs/mfa/mfainfo"))
        .query(&[
            ("domain", DOMAIN),
            ("account_name", account_name.as_str()),
            ("hl", "zh_CN"),
            ("all_secure", "1"),
        ])
        .send()
        .await;
    let r = format!("{}", rand::thread_rng().gen::<f64>());
    let body = client
        .get(format!("{entry}/login/sms/send_code.jsp"))
        .query(&[
            ("r", r.as_str()),
            ("domain", DOMAIN),
            ("account_name", account_name.as_str()),
        ])
        .send()
        .await
        .context("请求发送短信验证码失败")?
        .text()
        .await
        .context("读取短信发送结果失败")?;
    let result = body.trim().trim_matches('"');
    if result != "0" {
        bail!(
            "短信验证码发送失败（返回 {}），可能发送太频繁，请稍后再试",
            parse::snippet(result)
        );
    }
    let pending = Pending {
        entry: entry.to_string(),
        username: bare_id(username).to_string(),
        query: info.query.clone(),
        account_name,
        uid: info.uid.clone(),
        mobile: info.mobile.clone(),
        created_at: chrono::Utc::now().timestamp(),
    };
    write_json(&pending_path(store), &pending)?;
    store.save_cookie_store(cookies)?;
    Ok(())
}

/// 第 4 步：提交短信验证码（信任本设备）
async fn submit_sms(client: &reqwest::Client, pending: &Pending, code: &str) -> Result<LoginStep> {
    let code = code.trim();
    if !(4..=8).contains(&code.len()) || !code.chars().all(|c| c.is_ascii_digit()) {
        bail!("验证码应为 6 位数字");
    }
    let form = [
        ("domain", DOMAIN),
        ("account_name", pending.account_name.as_str()),
        ("_version", ""),
        ("autoEntry", "true"),
        ("code", code),
        ("pass_2fa", "1"),
        ("p", ""),
    ];
    let resp = client
        .post(format!(
            "{}/login/mobileLogin?{}",
            pending.entry, pending.query
        ))
        .form(&form)
        .send()
        .await
        .context("提交短信验证码失败")?;
    settle(client, resp).await
}

/// 确认会话可用：调一次最轻的 `mbox:getAllFolders`
async fn verify(client: &reqwest::Client, sess: &MailSession) -> Result<()> {
    let resp = client
        .post(format!("https://{}/js6/s", sess.host))
        .query(&[("func", "mbox:getAllFolders"), ("sid", sess.sid.as_str())])
        .header("accept", "application/json")
        .json(&json!({"order": "custom_virtual"}))
        .send()
        .await
        .context("校验邮箱会话失败")?;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    parse::parse_js6(status, &body)?;
    Ok(())
}

/// 登录成功：确定规范账号，保存会话
async fn finish(
    store: &Store,
    cookies: &Arc<CookieStoreMutex>,
    client: &reqwest::Client,
    username: &str,
    sid: String,
    host: String,
    uid_hint: Option<String>,
) -> Result<MailSession> {
    let mut device = load_device(store)?;
    let id = bare_id(username);
    let remembered =
        (device.username == id && !device.account.is_empty()).then(|| device.account.clone());
    let account = uid_hint
        .filter(|s| !s.is_empty())
        .or(cookie_value(cookies, "qiye_uid")?
            .map(|v| v.replace("%40", "@").trim_matches('"').to_string())
            .filter(|v| v.ends_with(&format!("@{DOMAIN}"))))
        .or(remembered)
        .map(|a| full_address(&a))
        .unwrap_or_else(|| full_address(id));

    let mut session = Session::new(sid.clone());
    session.uid = Some(account.clone());
    session.expires_at = Some(chrono::Utc::now().timestamp() + SESSION_TTL);
    session.extra = json!({ "host": host, "username": id });
    store.save_session(&session)?;
    store.save_cookie_store(cookies)?;

    device.username = id.to_string();
    device.account = account.clone();
    write_json(&device_path(store), &device)?;
    let _ = std::fs::remove_file(pending_path(store));

    let sess = MailSession { sid, host, account };
    if let Err(e) = verify(client, &sess).await {
        eprintln!("{} 登录成功但会话校验失败：{e:#}", "[!]".yellow());
    }
    Ok(sess)
}

fn print_done(sess: &MailSession, store: &Store, json_out: bool) {
    if json_out {
        println!(
            "{}",
            json!({"status": "ok", "account": sess.account, "host": sess.host})
        );
        return;
    }
    println!();
    println!("{} 北大学生邮箱登录成功！", "[done]".green().bold());
    println!("  邮箱     = {}", sess.account);
    println!("  服务器   = {}", sess.host);
    println!("  配置目录 = {}", store.config_dir().display());
}

fn print_sms_sent(mobile: &str, json_out: bool) {
    if json_out {
        println!(
            "{}",
            json!({
                "status": "sms_sent",
                "mobile": mobile,
                "next": "mail login -p --code <验证码>",
            })
        );
    } else {
        println!(
            "{} 短信验证码已发送到 {}。收到后运行：mail login -p --code <验证码>（10 分钟内有效）",
            "[sms]".cyan(),
            if mobile.is_empty() {
                "绑定手机"
            } else {
                mobile
            }
        );
    }
}

/// `mail login [-p] [--code N]`
pub async fn cmd_login(username: Option<&str>, code: Option<&str>, json_out: bool) -> Result<()> {
    let store = Store::new(APP_NAME)?;
    let cookies = store.load_cookie_store()?;
    let client = client::build_no_redirect(cookies.clone())?;

    if let Some(code) = code {
        return finish_sms(&store, &cookies, &client, code, json_out).await;
    }

    let cred = credential::resolve_credential(username)?;
    eprintln!("{} 登录北大学生邮箱...", "[+]".green());
    let (entry, step) =
        password_step(&store, &cookies, &client, &cred.username, &cred.password).await?;
    match step {
        LoginStep::Session { sid, host, uid } => {
            let sess = finish(&store, &cookies, &client, &cred.username, sid, host, uid).await?;
            print_done(&sess, &store, json_out);
            Ok(())
        }
        LoginStep::Sms(info) => {
            send_sms(&store, &cookies, &client, &entry, &cred.username, &info).await?;
            if !std::io::stdin().is_terminal() {
                print_sms_sent(&info.mobile, json_out);
                return Ok(());
            }
            let prompt = format!(
                "{} 本设备首次登录需要短信验证，验证码已发送到 {}\n验证码: ",
                "[sms]".cyan(),
                if info.mobile.is_empty() {
                    "绑定手机"
                } else {
                    &info.mobile
                }
            );
            let code = credential::resolve_sms_code(&prompt)?;
            finish_sms(&store, &cookies, &client, &code, json_out).await
        }
        LoginStep::Failed(why) => bail!("邮箱登录失败：{why}"),
        LoginStep::Next(_) => bail!("邮箱登录跳转异常"),
    }
}

async fn finish_sms(
    store: &Store,
    cookies: &Arc<CookieStoreMutex>,
    client: &reqwest::Client,
    code: &str,
    json_out: bool,
) -> Result<()> {
    let pending: Pending = read_json(&pending_path(store))?
        .ok_or_else(|| anyhow!("没有等待验证的登录，请先运行 `mail login -p`"))?;
    if chrono::Utc::now().timestamp() - pending.created_at > PENDING_TTL {
        let _ = std::fs::remove_file(pending_path(store));
        bail!("短信验证已超时，请重新运行 `mail login -p`");
    }
    match submit_sms(client, &pending, code).await? {
        LoginStep::Session { sid, host, uid } => {
            let uid = uid.or(Some(pending.uid.clone()));
            let sess = finish(store, cookies, client, &pending.username, sid, host, uid).await?;
            print_done(&sess, store, json_out);
            Ok(())
        }
        LoginStep::Sms(_) => {
            bail!("验证码错误或已过期，请检查后重试（或重新运行 `mail login -p` 获取新验证码）")
        }
        LoginStep::Failed(why) => bail!("短信验证失败：{why}"),
        LoginStep::Next(_) => bail!("短信验证跳转异常"),
    }
}

/// 会话失效时自动重新登录（只用 keyring / 环境变量里的凭据，不交互、不发短信）
pub async fn relogin_silently(
    store: &Store,
    cookies: &Arc<CookieStoreMutex>,
) -> Result<MailSession> {
    let has_env = std::env::var("PKU_USERNAME").is_ok_and(|v| !v.is_empty())
        && std::env::var("PKU_PASSWORD").is_ok_and(|v| !v.is_empty());
    if !has_env && !credential::keyring_has_credential().0 {
        bail!("邮箱会话已过期，请重新运行 `mail login -p`");
    }
    let cred = credential::resolve_credential(None)?;
    let client = client::build_no_redirect(cookies.clone())?;
    let (_, step) = password_step(store, cookies, &client, &cred.username, &cred.password).await?;
    match step {
        LoginStep::Session { sid, host, uid } => {
            finish(store, cookies, &client, &cred.username, sid, host, uid).await
        }
        LoginStep::Sms(_) => {
            bail!("邮箱会话已过期，且本设备需要重新短信验证，请运行 `mail login -p`")
        }
        LoginStep::Failed(why) => bail!("自动重新登录失败：{why}"),
        LoginStep::Next(_) => bail!("自动重新登录跳转异常"),
    }
}

pub fn status(json_out: bool) -> Result<()> {
    let store = Store::new(APP_NAME)?;
    let session = store.load_session()?;
    let sess = load_session(&store)?;
    let pending = read_json::<Pending>(&pending_path(&store))?;
    let trusted = cookie_value(&*store.load_cookie_store()?, "_sDeviceId")?.is_some();
    if json_out {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "logged_in": sess.is_some(),
                "account": sess.as_ref().map(|s| &s.account),
                "host": sess.as_ref().map(|s| &s.host),
                "created_at": session.as_ref().map(|s| s.created_at.to_rfc3339()),
                "expired": session.as_ref().map(Session::is_expired),
                "device_trusted": trusted,
                "sms_pending": pending.is_some(),
            }))?
        );
        return Ok(());
    }
    match (sess, session) {
        (Some(m), Some(s)) => {
            println!("{} 已登录", "●".green());
            println!("  邮箱     = {}", m.account);
            println!("  服务器   = {}", m.host);
            println!(
                "  登录时间 = {}",
                s.created_at
                    .with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M:%S")
            );
            if s.is_expired() {
                println!("  {}", "会话可能已过期，下次操作时会自动重新登录".yellow());
            }
        }
        _ => println!("{} 未登录。运行 `mail login -p` 开始。", "○".red()),
    }
    if pending.is_some() {
        println!(
            "  {}",
            "有一个等待短信验证的登录：mail login -p --code <验证码>".yellow()
        );
    }
    println!(
        "  本机设备 = {}",
        if trusted {
            "已信任（登录免短信）"
        } else {
            "未信任（首次登录需短信验证）"
        }
    );
    println!("  配置目录 = {}", store.config_dir().display());
    Ok(())
}

/// 退出登录：删除会话和 cookie，但保留设备 cookie，下次登录仍免短信
pub fn logout(json_out: bool) -> Result<()> {
    let store = Store::new(APP_NAME)?;
    let cookies = store.load_cookie_store()?;
    let kept: Vec<(String, String)> = cookies
        .lock()
        .map_err(|_| anyhow!("锁定 cookie store 失败"))?
        .iter_any()
        .filter(|c| DEVICE_COOKIES.contains(&c.name()))
        .map(|c| (c.name().to_string(), c.value().to_string()))
        .collect();
    store.clear()?;
    let _ = std::fs::remove_file(pending_path(&store));
    if !kept.is_empty() {
        let fresh = Arc::new(CookieStoreMutex::default());
        let url = url::Url::parse(COOKIE_URL)?;
        {
            let mut jar = fresh
                .lock()
                .map_err(|_| anyhow!("锁定 cookie store 失败"))?;
            for (name, value) in &kept {
                jar.parse(
                    &format!("{name}={value}; Domain=qiye.163.com; Path=/; Max-Age=2592000"),
                    &url,
                )
                .context("保留设备 cookie 失败")?;
            }
        }
        store.save_cookie_store(&fresh)?;
    }
    if json_out {
        println!("{}", json!({"status": "logged_out"}));
    } else {
        println!(
            "{} 已清除本地会话（保留设备信任，下次登录免短信）",
            "✓".green()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_jsonp() {
        assert_eq!(bare_id("2100012345@stu.pku.edu.cn"), "2100012345");
        assert_eq!(full_address("zs"), "zs@stu.pku.edu.cn");
        assert_eq!(full_address("zs@stu.pku.edu.cn"), "zs@stu.pku.edu.cn");
        assert_eq!(strip_jsonp(r#"cb({"code":200})"#), r#"{"code":200}"#);
        assert_eq!(strip_jsonp(r#"{"a":"(x)"}"#), r#"{"a":"(x)"}"#);
    }
}
