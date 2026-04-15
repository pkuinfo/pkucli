//! 可插拔验证码识别模块
//!
//! 支持四种模式：
//! - Manual: 终端显示验证码图片，用户手动输入
//! - TTShiTu: ttshitu.com 付费 API
//! - Utool: utool.pro 免费 API
//! - Yunma: 云码 (jfbym.com) 付费 API，关注公众号可领免费积分

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};

/// 验证码识别后端配置（持久化到 config.toml）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CaptchaConfig {
    /// 手动输入
    #[serde(rename = "manual")]
    #[default]
    Manual,
    /// TTShiTu API
    #[serde(rename = "ttshitu")]
    TTShiTu {
        username: String,
        password: String,
    },
    /// UTOOL Pro API（免费，无需凭证）
    #[serde(rename = "utool")]
    Utool,
    /// 云码 API（需要 token）
    #[serde(rename = "yunma")]
    Yunma {
        token: String,
    },
}

impl std::fmt::Display for CaptchaConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Manual => write!(f, "手动输入"),
            Self::TTShiTu { username, .. } => write!(f, "TTShiTu ({username})"),
            Self::Utool => write!(f, "UTOOL Pro (免费)"),
            Self::Yunma { .. } => write!(f, "云码 (jfbym.com)"),
        }
    }
}

/// 识别验证码图片
///
/// `image_bytes`: JPEG 格式的原始字节
pub async fn recognize(
    client: &reqwest::Client,
    config: &CaptchaConfig,
    image_bytes: &[u8],
    config_dir: &std::path::Path,
) -> Result<String> {
    match config {
        CaptchaConfig::Manual => recognize_manual(image_bytes, config_dir),
        CaptchaConfig::TTShiTu { username, password } => {
            recognize_ttshitu(client, username, password, image_bytes).await
        }
        CaptchaConfig::Utool => recognize_utool(client, image_bytes).await,
        CaptchaConfig::Yunma { token } => {
            recognize_yunma(client, token, image_bytes).await
        }
    }
}

// ─── 手动模式 ──────────────────────────────────────────────────

fn recognize_manual(image_bytes: &[u8], config_dir: &std::path::Path) -> Result<String> {
    // 将验证码保存为临时文件
    let captcha_path = config_dir.join("captcha.jpg");
    std::fs::write(&captcha_path, image_bytes)
        .context("保存验证码图片失败")?;

    // 在终端渲染验证码
    println!();
    let viuer_conf = viuer::Config {
        absolute_offset: false,
        width: Some(40),
        height: Some(6),
        ..Default::default()
    };
    if viuer::print_from_file(&captcha_path, &viuer_conf).is_err() {
        println!("  (无法在终端渲染，请手动打开: {})", captcha_path.display());
    }

    print!("请输入验证码: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let code = input.trim().to_string();

    if code.is_empty() {
        return Err(anyhow!("验证码不能为空"));
    }

    Ok(code)
}

// ─── TTShiTu ───────────────────────────────────────────────────

const TTSHITU_API: &str = "http://api.ttshitu.com/base64";

#[derive(Deserialize)]
struct TTShiTuResp {
    data: Option<TTShiTuData>,
}

#[derive(Deserialize)]
struct TTShiTuData {
    result: String,
}

async fn recognize_ttshitu(
    client: &reqwest::Client,
    username: &str,
    password: &str,
    image_bytes: &[u8],
) -> Result<String> {
    use base64::Engine;

    // 将 JPEG 转为 base64
    let b64 = base64::engine::general_purpose::STANDARD.encode(image_bytes);

    let body = serde_json::json!({
        "username": username,
        "password": password,
        "typeid": 3,
        "image": b64,
    });

    let resp: TTShiTuResp = client
        .post(TTSHITU_API)
        .json(&body)
        .send()
        .await
        .context("TTShiTu API 请求失败")?
        .json()
        .await
        .context("TTShiTu 响应解析失败")?;

    let result = resp
        .data
        .ok_or_else(|| anyhow!("TTShiTu 返回数据为空"))?
        .result;

    Ok(result)
}

// ─── UTOOL Pro ─────────────────────────────────────────────────

const UTOOL_API: &str = "https://api.leepow.com/verifycode";

#[derive(Deserialize)]
struct UtoolResp {
    code: i32,
    data: Option<String>,
    msg: Option<String>,
}

async fn recognize_utool(
    client: &reqwest::Client,
    image_bytes: &[u8],
) -> Result<String> {
    use base64::Engine;

    let b64 = base64::engine::general_purpose::STANDARD.encode(image_bytes);

    let body = serde_json::json!({
        "image": b64,
    });

    let resp: UtoolResp = client
        .post(UTOOL_API)
        .json(&body)
        .send()
        .await
        .context("UTOOL API 请求失败")?
        .json()
        .await
        .context("UTOOL 响应解析失败")?;

    if resp.code != 0 {
        return Err(anyhow!(
            "UTOOL 识别失败: {}",
            resp.msg.unwrap_or_else(|| "未知错误".to_string())
        ));
    }

    resp.data
        .ok_or_else(|| anyhow!("UTOOL 返回数据为空"))
}

// ─── 云码 (jfbym.com) ──────────────────────────────────────────

const YUNMA_API: &str = "http://api.jfbym.com/api/YmServer/customApi";

#[derive(Deserialize)]
struct YunmaResp {
    code: i32,
    msg: String,
    data: Option<YunmaData>,
}

#[derive(Deserialize)]
struct YunmaData {
    data: String,
}

async fn recognize_yunma(
    client: &reqwest::Client,
    token: &str,
    image_bytes: &[u8],
) -> Result<String> {
    use base64::Engine;

    let b64 = base64::engine::general_purpose::STANDARD.encode(image_bytes);

    let body = serde_json::json!({
        "image": b64,
        "token": token,
        "type": "10110",
    });

    let resp: YunmaResp = client
        .post(YUNMA_API)
        .json(&body)
        .send()
        .await
        .context("云码 API 请求失败")?
        .json()
        .await
        .context("云码响应解析失败")?;

    if resp.code != 10000 {
        return Err(anyhow!("云码识别失败: {}", resp.msg));
    }

    resp.data
        .map(|d| d.data)
        .ok_or_else(|| anyhow!("云码返回数据为空"))
}
