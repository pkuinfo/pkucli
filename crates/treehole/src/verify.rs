//! 树洞短信验证流程
//!
//! 树洞在首次登录或定期（约30天）要求用户完成短信验证。
//! 此验证与 IAAA 无关，是树洞自身的安全机制。
//!
//! 流程：
//! 1. 访问需认证接口返回 code=40002 → 说明需要短信验证
//! 2. POST /chapi/api/jwt_send_msg  {} → 发送验证码到绑定手机
//! 3. POST /chapi/api/jwt_msg_verify {"valid_code": "<code>"} → 提交验证码
//!
//! 另外还存在 OTP 动态口令验证（/api/check_otp），暂不实现。

use crate::client::TREEHOLE_BASE;
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use info_common::credential;
use serde::Deserialize;

/// 树洞 API 需要短信验证时的错误码
pub const CODE_SMS_REQUIRED: i64 = 40002;

#[derive(Deserialize)]
struct ApiResponse {
    #[serde(default)]
    code: i64,
    success: bool,
    message: String,
}

/// 检查是否需要短信验证，如需要则引导用户完成验证
pub async fn check_and_verify(
    client: &reqwest::Client,
    jwt_token: &str,
    uuid: &str,
) -> Result<()> {
    // 用帖子接口探测认证状态（该接口对短信验证敏感，un_read 则不敏感）
    let resp: ApiResponse = client
        .get(format!(
            "{TREEHOLE_BASE}/chapi/api/v3/hole/list_comments?page=1&limit=1&comment_limit=0&comment_stream=1"
        ))
        .header("authorization", format!("Bearer {jwt_token}"))
        .header("uuid", uuid)
        .send()
        .await
        .context("验证登录状态失败")?
        .json()
        .await
        .context("解析验证响应失败")?;

    if resp.code == CODE_SMS_REQUIRED {
        println!(
            "{} 需要短信验证（首次登录或定期验证）",
            "[!]".yellow()
        );
        handle_sms_verification(client, jwt_token, uuid).await
    } else if !resp.success {
        Err(anyhow!(
            "登录验证失败: code={}, message={}",
            resp.code,
            resp.message
        ))
    } else {
        Ok(())
    }
}

/// 发送验证码 → 等待用户输入 → 提交验证码
async fn handle_sms_verification(
    client: &reqwest::Client,
    jwt_token: &str,
    uuid: &str,
) -> Result<()> {
    // 1. 确认发送
    if !credential::confirm_send_sms("是否发送短信验证码? [Y/n] ")? {
        return Err(anyhow!("用户取消短信验证"));
    }

    // 2. 发送验证码
    let send_resp: ApiResponse = client
        .post(format!("{TREEHOLE_BASE}/chapi/api/jwt_send_msg"))
        .header("authorization", format!("Bearer {jwt_token}"))
        .header("uuid", uuid)
        .json(&serde_json::json!({}))
        .send()
        .await
        .context("发送短信验证码失败")?
        .json()
        .await?;

    if !send_resp.success {
        return Err(anyhow!("发送短信失败: {}", send_resp.message));
    }
    println!("{} 验证码已发送到绑定手机", "✓".green());

    // 3. 输入验证码
    let code = credential::resolve_sms_code("请输入验证码: ")?;

    // 4. 提交验证码
    let verify_resp: ApiResponse = client
        .post(format!("{TREEHOLE_BASE}/chapi/api/jwt_msg_verify"))
        .header("authorization", format!("Bearer {jwt_token}"))
        .header("uuid", uuid)
        .json(&serde_json::json!({ "valid_code": code }))
        .send()
        .await
        .context("提交验证码失败")?
        .json()
        .await?;

    if verify_resp.success {
        println!("{} 短信验证通过", "✓".green());
        Ok(())
    } else {
        Err(anyhow!("短信验证失败: {}", verify_resp.message))
    }
}
