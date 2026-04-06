//! PKU IAAA (统一身份认证) 登录模块
//!
//! 支持两种登录方式：
//! 1. 用户名 + 密码（RSA 加密）
//! 2. 北京大学 App 扫码登录
//!
//! 登录成功后返回 IAAA token，由调用方自行完成后续的服务鉴权。

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use colored::Colorize;
use rand::Rng;
use rsa::{pkcs8::DecodePublicKey, Pkcs1v15Encrypt, RsaPublicKey};
use serde::Deserialize;
use std::{fs, path::Path, time::Duration};

const IAAA_BASE: &str = "https://iaaa.pku.edu.cn/iaaa";

/// IAAA 登录成功后的结果
#[derive(Debug, Clone)]
pub struct IaaaToken {
    pub token: String,
}

/// IAAA 登录配置
#[derive(Debug, Clone)]
pub struct IaaaConfig {
    /// 目标应用 ID（例如 "PKU Helper"）
    pub app_id: String,
    /// 登录完成后的回调 URL
    pub redirect_url: String,
}

#[derive(Deserialize)]
struct PublicKeyResp {
    success: bool,
    key: Option<String>,
}

#[derive(Deserialize)]
struct LoginResp {
    success: bool,
    token: Option<String>,
    errors: Option<LoginError>,
}

#[derive(Deserialize)]
struct LoginError {
    code: Option<String>,
    msg: Option<String>,
}

/// 使用用户名和密码进行 IAAA 登录
pub async fn login_password(
    client: &reqwest::Client,
    config: &IaaaConfig,
    username: &str,
    password: &str,
) -> Result<IaaaToken> {
    // Step 1: 获取 RSA 公钥
    println!("{} 获取 IAAA RSA 公钥...", "[1/3]".green());
    let pub_key_url = format!("{IAAA_BASE}/getPublicKey.do");
    let pk_resp: PublicKeyResp = client
        .get(&pub_key_url)
        .header("x-requested-with", "XMLHttpRequest")
        .header("referer", format!("{IAAA_BASE}/oauth.jsp"))
        .send()
        .await
        .context("获取公钥请求失败")?
        .json()
        .await
        .context("解析公钥响应失败")?;

    if !pk_resp.success {
        return Err(anyhow!("获取公钥失败"));
    }
    let pem = pk_resp.key.ok_or_else(|| anyhow!("公钥为空"))?;

    // Step 2: RSA 加密密码
    println!("{} 加密密码...", "[2/3]".green());
    let encrypted = encrypt_password(&pem, password)?;

    // Step 3: 提交登录
    println!("{} 提交登录...", "[3/3]".green());
    let login_url = format!("{IAAA_BASE}/oauthlogin.do");
    let form = [
        ("appid", config.app_id.as_str()),
        ("userName", username),
        ("password", encrypted.as_str()),
        ("randCode", ""),
        ("smsCode", ""),
        ("otpCode", ""),
        ("redirUrl", config.redirect_url.as_str()),
    ];

    let resp: LoginResp = client
        .post(&login_url)
        .header("x-requested-with", "XMLHttpRequest")
        .header("referer", format!("{IAAA_BASE}/oauth.jsp"))
        .header("content-type", "application/x-www-form-urlencoded")
        .form(&form)
        .send()
        .await
        .context("登录请求失败")?
        .json()
        .await
        .context("解析登录响应失败")?;

    if resp.success {
        let token = resp.token.ok_or_else(|| anyhow!("登录成功但 token 为空"))?;
        Ok(IaaaToken { token })
    } else {
        let err = resp.errors.unwrap_or(LoginError {
            code: None,
            msg: None,
        });
        Err(anyhow!(
            "IAAA 登录失败: code={}, msg={}",
            err.code.unwrap_or_default(),
            err.msg.unwrap_or_else(|| "未知错误".to_string())
        ))
    }
}

/// 使用北京大学 App 扫码进行 IAAA 登录
pub async fn login_qrcode(
    client: &reqwest::Client,
    config: &IaaaConfig,
    qr_save_dir: &Path,
    qr_mode: crate::qr::QrDisplayMode,
) -> Result<IaaaToken> {
    // Step 0: 先访问 oauth.jsp 建立 JSESSIONID 会话
    println!("{} 建立 IAAA 会话...", "[1/4]".green());
    let oauth_url = format!(
        "{IAAA_BASE}/oauth.jsp?appID={}&appName=&redirectUrl={}",
        urlencoding(&config.app_id),
        urlencoding(&config.redirect_url),
    );
    let _ = client
        .get(&oauth_url)
        .send()
        .await
        .context("访问 IAAA 登录页失败")?
        .bytes()
        .await;

    // Step 1: 获取二维码图片
    println!("{} 获取 IAAA 扫码二维码...", "[2/4]".green());
    let qr_url = format!(
        "{IAAA_BASE}/genQRCode.do?userName=&_rand={}&appId={}",
        rand::thread_rng().gen::<f64>(),
        urlencoding(&config.app_id),
    );
    let qr_bytes = client
        .get(&qr_url)
        .header("referer", format!("{IAAA_BASE}/oauth.jsp"))
        .send()
        .await
        .context("获取二维码失败")?
        .bytes()
        .await?;

    let qr_path = qr_save_dir.join("iaaa-qrcode.png");
    fs::write(&qr_path, &qr_bytes)
        .with_context(|| format!("保存二维码失败: {}", qr_path.display()))?;

    // Step 2: 展示二维码
    println!(
        "{} 请使用「{}」扫描下方二维码：",
        "[3/4]".green(),
        "北京大学 App".bold()
    );
    if let Err(e) = crate::qr::render_qr_image(&qr_path, qr_mode) {
        println!(
            "{} 终端无法渲染二维码（{e}），请手动打开: {}",
            "[warn]".yellow(),
            qr_path.display()
        );
    } else {
        println!(
            "    {} {}",
            "（二维码已保存到）".dimmed(),
            qr_path.display()
        );
    }
    println!();

    // Step 3: 轮询等待扫码
    println!("{} 等待扫码确认...", "[4/4]".green());
    let poll_url = format!("{IAAA_BASE}/oauthlogin4QRCode.do");
    let form = [
        ("appId", "PKUApp"),
        ("issuerAppId", "iaaa"),
        ("targetAppId", config.app_id.as_str()),
        ("redirectUrl", config.redirect_url.as_str()),
    ];

    for attempt in 0..60 {
        let resp: LoginResp = client
            .post(&poll_url)
            .header("x-requested-with", "XMLHttpRequest")
            .header("referer", format!("{IAAA_BASE}/oauth.jsp"))
            .form(&form)
            .send()
            .await
            .context("轮询请求失败")?
            .json()
            .await
            .context("解析轮询响应失败")?;

        if resp.success {
            let token = resp
                .token
                .ok_or_else(|| anyhow!("扫码成功但 token 为空"))?;
            println!("   {} 扫码登录成功", "✓".green());
            return Ok(IaaaToken { token });
        }

        // E10 = 无有效绑定 = 还未扫码，继续等
        let code = resp
            .errors
            .as_ref()
            .and_then(|e| e.code.as_deref())
            .unwrap_or("");

        match code {
            "E10" => {
                if attempt == 0 {
                    println!("   等待扫描...");
                }
            }
            "E99" => {
                return Err(anyhow!("二维码已失效，请重试"));
            }
            _ => {
                let msg = resp
                    .errors
                    .as_ref()
                    .and_then(|e| e.msg.as_deref())
                    .unwrap_or("未知错误");
                if code == "E02" {
                    return Err(anyhow!("账号需要激活: {msg}"));
                }
                // 其他错误继续轮询
                tracing::debug!("QR poll: code={code}, msg={msg}");
            }
        }

        let delay = 3000 + rand::thread_rng().gen_range(0..500);
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }

    Err(anyhow!("扫码超时（3分钟），请重新尝试"))
}

/// 使用 RSA 公钥加密密码
fn encrypt_password(pem: &str, password: &str) -> Result<String> {
    let public_key = RsaPublicKey::from_public_key_pem(pem)
        .context("解析 RSA 公钥失败")?;
    let mut rng = rand::thread_rng();
    let encrypted = public_key
        .encrypt(&mut rng, Pkcs1v15Encrypt, password.as_bytes())
        .context("RSA 加密密码失败")?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&encrypted))
}

fn urlencoding(s: &str) -> String {
    s.replace(' ', "%20")
}
