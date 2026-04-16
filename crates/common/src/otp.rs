//! PKU IAAA OTP (手机令牌) 模块
//!
//! 功能：
//! 1. TOTP 码生成（标准 RFC 6238: HMAC-SHA1, 6 位, 30 秒步长）
//! 2. OTP secret 本地持久化
//! 3. OTP 绑定流程（auth4Bind → 短信验证 → genOtpKey → userBind）

use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use cookie_store::CookieStore;
use hmac::{Hmac, Mac};
use reqwest_cookie_store::CookieStoreMutex;
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::credential;

type HmacSha1 = Hmac<Sha1>;

const IAAA_BASE: &str = "https://iaaa.pku.edu.cn/iaaa";
const TOTP_PERIOD: u64 = 30;
const TOTP_DIGITS: u32 = 6;

// ─── TOTP 码生成 ──────────────────────────────────────────────

/// 从 Base32 编码的 secret 生成当前 TOTP 码
pub fn generate_totp(secret_base32: &str) -> Result<String> {
    let secret = data_encoding::BASE32_NOPAD
        .decode(secret_base32.trim().as_bytes())
        .or_else(|_| data_encoding::BASE32.decode(secret_base32.trim().as_bytes()))
        .context("Base32 解码 TOTP secret 失败")?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("获取系统时间失败")?;
    let counter = now.as_secs() / TOTP_PERIOD;

    generate_hotp(&secret, counter)
}

/// HOTP 算法 (RFC 4226)
fn generate_hotp(secret: &[u8], counter: u64) -> Result<String> {
    let counter_bytes = counter.to_be_bytes();

    let mut mac = HmacSha1::new_from_slice(secret).context("HMAC-SHA1 密钥初始化失败")?;
    mac.update(&counter_bytes);
    let result = mac.finalize().into_bytes();

    // Dynamic truncation
    let offset = (result[19] & 0x0f) as usize;
    let code = u32::from_be_bytes([
        result[offset] & 0x7f,
        result[offset + 1],
        result[offset + 2],
        result[offset + 3],
    ]);

    let otp = code % 10u32.pow(TOTP_DIGITS);
    Ok(format!("{otp:0>width$}", width = TOTP_DIGITS as usize))
}

// ─── OTP 配置持久化 ──────────────────────────────────────────

/// OTP 配置（存储在各 crate 的配置目录下）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtpConfig {
    /// TOTP secret (Base32 编码)
    pub secret: String,
    /// 关联的用户 ID
    pub user_id: String,
    /// 关联的用户姓名
    pub user_name: String,
}

const OTP_CONFIG_FILE: &str = "otp.json";

/// 从配置目录加载 OTP 配置
pub fn load_otp_config(config_dir: &Path) -> Result<Option<OtpConfig>> {
    let path = config_dir.join(OTP_CONFIG_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let bytes =
        std::fs::read(&path).with_context(|| format!("读取 OTP 配置失败: {}", path.display()))?;
    let config: OtpConfig = serde_json::from_slice(&bytes)
        .with_context(|| format!("解析 OTP 配置失败: {}", path.display()))?;
    Ok(Some(config))
}

/// 保存 OTP 配置到配置目录
pub fn save_otp_config(config_dir: &Path, config: &OtpConfig) -> Result<()> {
    let path = config_dir.join(OTP_CONFIG_FILE);
    let data = serde_json::to_vec_pretty(config)?;
    std::fs::write(&path, data)
        .with_context(|| format!("写入 OTP 配置失败: {}", path.display()))?;
    Ok(())
}

/// 删除 OTP 配置
pub fn clear_otp_config(config_dir: &Path) -> Result<()> {
    let path = config_dir.join(OTP_CONFIG_FILE);
    if path.exists() {
        std::fs::remove_file(&path)
            .with_context(|| format!("删除 OTP 配置失败: {}", path.display()))?;
    }
    Ok(())
}

/// 获取当前 TOTP 码（如果已配置）
pub fn get_current_otp(config_dir: &Path) -> Result<Option<String>> {
    match load_otp_config(config_dir)? {
        Some(config) => {
            let code = generate_totp(&config.secret)?;
            Ok(Some(code))
        }
        None => Ok(None),
    }
}

// ─── OTP 绑定流程 ─────────────────────────────────────────────

#[derive(Deserialize)]
struct AuthBindResp {
    success: bool,
    errors: Option<AuthBindError>,
}

#[derive(Deserialize)]
struct AuthBindError {
    msg: Option<String>,
}

#[derive(Deserialize)]
struct GenOtpKeyResp {
    success: bool,
    #[serde(rename = "personId")]
    person_id: Option<String>,
    #[serde(rename = "personName")]
    person_name: Option<String>,
    #[serde(rename = "secKey")]
    sec_key: Option<String>,
    #[serde(rename = "errMsg")]
    err_msg: Option<String>,
}

#[derive(Deserialize)]
struct SmsResp {
    success: bool,
    #[serde(rename = "mobileMask")]
    mobile_mask: Option<String>,
    #[serde(rename = "errMsg")]
    err_msg: Option<String>,
}

#[derive(Deserialize)]
struct CheckSmsResp {
    success: bool,
    message: Option<String>,
}

#[derive(Deserialize)]
struct UserBindResp {
    success: bool,
    #[serde(rename = "errMsg")]
    err_msg: Option<String>,
}

const OTP_PENDING_STATE_FILE: &str = "otp_pending.json";
const OTP_PENDING_COOKIES_FILE: &str = "otp_pending_cookies.json";

#[derive(Serialize, Deserialize)]
struct OtpPendingState {
    username: String,
}

fn pending_state_path(config_dir: &Path) -> std::path::PathBuf {
    config_dir.join(OTP_PENDING_STATE_FILE)
}

fn pending_cookies_path(config_dir: &Path) -> std::path::PathBuf {
    config_dir.join(OTP_PENDING_COOKIES_FILE)
}

fn save_pending(
    config_dir: &Path,
    username: &str,
    cookie_store: &Arc<CookieStoreMutex>,
) -> Result<()> {
    std::fs::create_dir_all(config_dir)?;

    let state = OtpPendingState {
        username: username.to_string(),
    };
    let state_json = serde_json::to_vec_pretty(&state)?;
    std::fs::write(pending_state_path(config_dir), state_json).context("保存 OTP 绑定状态失败")?;

    let cookies_path = pending_cookies_path(config_dir);
    let file = std::fs::File::create(&cookies_path)
        .with_context(|| format!("写入 cookie 文件失败: {}", cookies_path.display()))?;
    let guard = cookie_store
        .lock()
        .map_err(|e| anyhow!("锁定 cookie store 失败: {e}"))?;
    let mut writer = std::io::BufWriter::new(file);
    cookie_store::serde::json::save_incl_expired_and_nonpersistent(&guard, &mut writer)
        .map_err(|e| anyhow!("序列化 cookie 文件失败: {e}"))?;
    Ok(())
}

fn load_pending(config_dir: &Path) -> Result<(String, Arc<CookieStoreMutex>)> {
    let state_path = pending_state_path(config_dir);
    if !state_path.exists() {
        return Err(anyhow!(
            "未找到 OTP 绑定会话。请先运行 `otp bind --send` 发送短信验证码"
        ));
    }

    let state_bytes = std::fs::read(&state_path).context("读取 OTP 绑定状态失败")?;
    let state: OtpPendingState =
        serde_json::from_slice(&state_bytes).context("解析 OTP 绑定状态失败")?;

    let cookies_path = pending_cookies_path(config_dir);
    let file = std::fs::File::open(&cookies_path)
        .with_context(|| format!("打开 cookie 文件失败: {}", cookies_path.display()))?;
    let store = cookie_store::serde::json::load(std::io::BufReader::new(file))
        .map_err(|e| anyhow!("解析 cookie 文件失败: {e}"))?;

    Ok((state.username, Arc::new(CookieStoreMutex::new(store))))
}

fn clear_pending(config_dir: &Path) {
    let _ = std::fs::remove_file(pending_state_path(config_dir));
    let _ = std::fs::remove_file(pending_cookies_path(config_dir));
}

fn build_bind_client(cookie_store: Arc<CookieStoreMutex>) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .cookie_provider(cookie_store)
        .build()
        .context("构建 HTTP 客户端失败")
}

/// 阶段 1：认证身份并发送短信验证码，保存会话到本地
///
/// 适用于 AI Agent 场景：先调用此函数触发短信，等用户告知验证码后，
/// 再调用 `bind_otp_verify` 完成绑定。
pub async fn bind_otp_send_sms(config_dir: &Path, username: Option<&str>) -> Result<()> {
    let cookie_store = Arc::new(CookieStoreMutex::new(CookieStore::default()));
    let client = build_bind_client(cookie_store.clone())?;

    let cred = credential::resolve_credential(username)?;
    auth_and_send_sms(&client, &cred.username, &cred.password).await?;

    save_pending(config_dir, &cred.username, &cookie_store)?;

    println!();
    println!("{} 短信已发送，会话已保存。收到验证码后运行:", "[i]".cyan());
    println!("  {} --verify <CODE>", "otp bind".bold());
    Ok(())
}

/// 阶段 2：用已保存的会话和短信验证码完成 OTP 绑定
///
/// 必须先通过 `bind_otp_send_sms` 创建绑定会话。
pub async fn bind_otp_verify(config_dir: &Path, sms_code: &str) -> Result<OtpConfig> {
    let (username, cookie_store) = load_pending(config_dir)?;
    let client = build_bind_client(cookie_store)?;

    let result = verify_sms_and_finalize(&client, config_dir, &username, sms_code).await;

    // 仅在成功时清理 pending 状态；失败时保留，方便用户用新的 code 重试
    if result.is_ok() {
        clear_pending(config_dir);
    }

    result
}

/// 完整的 OTP 绑定流程（交互式）
///
/// 1. 用户名+密码 → auth4Bind
/// 2. 发送短信验证码 → 用户输入
/// 3. 验证短信码 → checkSms
/// 4. 获取 TOTP secret → genOtpKey
/// 5. 生成 OTP 码验证 → userBind
/// 6. 保存 secret 到本地
pub async fn bind_otp_interactive(config_dir: &Path, username: Option<&str>) -> Result<OtpConfig> {
    let cookie_store = Arc::new(CookieStoreMutex::new(CookieStore::default()));
    let client = build_bind_client(cookie_store)?;

    let cred = credential::resolve_credential(username)?;
    auth_and_send_sms(&client, &cred.username, &cred.password).await?;

    let sms_code = credential::resolve_sms_code("请输入短信验证码: ")?;
    verify_sms_and_finalize(&client, config_dir, &cred.username, &sms_code).await
}

/// 内部：执行身份验证并发送短信验证码（Steps 1-2）
async fn auth_and_send_sms(client: &reqwest::Client, username: &str, password: &str) -> Result<()> {
    // Step 1: auth4Bind 验证身份
    println!("{} 验证身份...", "[1/5]".green());
    let auth_url = format!("{IAAA_BASE}/auth4Bind.do");
    let auth_resp: AuthBindResp = client
        .post(&auth_url)
        .form(&[
            ("userName", username),
            ("password", password),
            ("randCode", ""),
        ])
        .send()
        .await
        .context("auth4Bind 请求失败")?
        .json()
        .await
        .context("auth4Bind 响应解析失败")?;

    if !auth_resp.success {
        let msg = auth_resp
            .errors
            .and_then(|e| e.msg)
            .unwrap_or_else(|| "未知错误".to_string());
        return Err(anyhow!("身份验证失败: {msg}"));
    }

    // Step 2: 发送短信验证码
    println!("{} 发送短信验证码...", "[2/5]".green());
    let sms_url = format!(
        "{IAAA_BASE}/pageFlows/identity/otpBind/sendSMSCodeBind.do?_rand={}",
        rand::random::<f64>()
    );
    let sms_resp: SmsResp = client
        .get(&sms_url)
        .send()
        .await
        .context("发送短信验证码失败")?
        .json()
        .await
        .context("短信响应解析失败")?;

    if !sms_resp.success {
        let msg = sms_resp.err_msg.unwrap_or_else(|| "未知错误".to_string());
        return Err(anyhow!("发送短信失败: {msg}"));
    }

    let mobile = sms_resp.mobile_mask.unwrap_or_default();
    println!("  验证码已发送至 {mobile}");
    Ok(())
}

/// 内部：验证短信码、获取 secret、完成绑定、保存配置（Steps 3-6）
async fn verify_sms_and_finalize(
    client: &reqwest::Client,
    config_dir: &Path,
    username: &str,
    sms_code: &str,
) -> Result<OtpConfig> {
    // Step 3: 验证短信码
    println!("{} 验证短信码...", "[3/5]".green());
    let check_url = format!("{IAAA_BASE}/pageFlows/identity/otpBind/checkSms.do");
    let check_resp: CheckSmsResp = client
        .post(&check_url)
        .form(&[("userId", username), ("smsCode", sms_code)])
        .send()
        .await
        .context("短信验证请求失败")?
        .json()
        .await
        .context("短信验证响应解析失败")?;

    if !check_resp.success {
        let msg = check_resp
            .message
            .unwrap_or_else(|| "验证码错误".to_string());
        return Err(anyhow!("短信验证失败: {msg}"));
    }

    // Step 5: 获取 TOTP secret
    println!("{} 获取令牌密钥...", "[4/5]".green());
    let gen_url = format!(
        "{IAAA_BASE}/pageFlows/identity/otpBind/genOtpKey.do?_rand={}",
        rand::random::<f64>()
    );
    let gen_resp: GenOtpKeyResp = client
        .get(&gen_url)
        .send()
        .await
        .context("获取 OTP 密钥失败")?
        .json()
        .await
        .context("OTP 密钥响应解析失败")?;

    if !gen_resp.success {
        let msg = gen_resp.err_msg.unwrap_or_else(|| "会话过期".to_string());
        return Err(anyhow!("获取 OTP 密钥失败: {msg}"));
    }

    let person_id = gen_resp
        .person_id
        .ok_or_else(|| anyhow!("响应缺少 personId"))?;
    let person_name = gen_resp
        .person_name
        .ok_or_else(|| anyhow!("响应缺少 personName"))?;
    let sec_key = gen_resp.sec_key.ok_or_else(|| anyhow!("响应缺少 secKey"))?;

    // Step 6: 生成 OTP 码并完成绑定
    println!("{} 完成绑定...", "[5/5]".green());
    let otp_code = generate_totp(&sec_key)?;

    let bind_url = format!("{IAAA_BASE}/pageFlows/identity/otpBind/userBind.do");
    let bind_resp: UserBindResp = client
        .post(&bind_url)
        .form(&[
            ("userId", person_id.as_str()),
            ("otpCode", otp_code.as_str()),
        ])
        .send()
        .await
        .context("绑定请求失败")?
        .json()
        .await
        .context("绑定响应解析失败")?;

    if !bind_resp.success {
        let msg = bind_resp.err_msg.unwrap_or_else(|| "绑定失败".to_string());
        return Err(anyhow!("OTP 绑定失败: {msg}"));
    }

    // Step 7: 保存到本地
    let config = OtpConfig {
        secret: sec_key,
        user_id: person_id,
        user_name: person_name.clone(),
    };
    save_otp_config(config_dir, &config)?;

    let otp_uri = format!(
        "otpauth://totp/iaaa.pku.edu.cn:{}?secret={}&issuer=iaaa.pku.edu.cn",
        config.user_id, config.secret,
    );

    println!();
    println!("{} OTP 绑定成功！", "✓".green().bold());
    println!("  用户:   {} ({})", person_name, config.user_id);
    println!("  Secret: {}", config.secret.bold());
    println!("  URI:    {}", otp_uri);
    println!("  配置:   {}", config_dir.display());
    println!();
    println!(
        "{}",
        "提示: 将上面的 Secret 或 URI 手动添加到手机 FreeOTP/Google Authenticator，即可多设备共用。"
            .yellow()
    );

    Ok(config)
}

/// 手动设置 OTP secret（用户已有 secret 的情况）
pub fn set_otp_secret(config_dir: &Path, secret: &str, user_id: &str) -> Result<OtpConfig> {
    // 验证 secret 是否有效
    generate_totp(secret).context("无效的 TOTP secret")?;

    let config = OtpConfig {
        secret: secret.to_string(),
        user_id: user_id.to_string(),
        user_name: String::new(),
    };
    save_otp_config(config_dir, &config)?;

    println!("{} OTP secret 已保存", "✓".green());
    Ok(config)
}
