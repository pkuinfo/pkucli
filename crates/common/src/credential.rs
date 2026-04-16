//! 统一凭据解析模块
//!
//! 按以下优先级自动获取用户凭据：
//! 1. 现有 session（未过期直接复用）
//! 2. OS 系统密钥链（keyring）
//! 3. 环境变量（`PKU_USERNAME` / `PKU_PASSWORD`）
//! 4. 交互式输入（兜底）
//!
//! 密码 **永远不落盘** —— keyring 由操作系统加密管理，环境变量仅存在于进程内存中。

use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use std::io::{self, Write};

use crate::session::Store;

const ENV_USERNAME: &str = "PKU_USERNAME";
const ENV_PASSWORD: &str = "PKU_PASSWORD";
const ENV_SMS_CODE: &str = "PKU_SMS_CODE";
const KEYRING_SERVICE: &str = "info-pku";

/// 凭据来源
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialSource {
    Keyring,
    EnvVar,
    Interactive,
}

/// 已解析的凭据
#[derive(Debug, Clone)]
pub struct Credential {
    pub username: String,
    pub password: String,
    pub source: CredentialSource,
}

/// 会话检查结果
#[derive(Debug)]
pub enum SessionStatus {
    /// 有效且未过期
    Valid,
    /// 已过期
    Expired,
    /// 不存在
    NotFound,
}

/// 检查指定 app 的 session 是否有效
pub fn check_session(app_name: &str) -> Result<SessionStatus> {
    let store = Store::new(app_name)?;
    match store.load_session()? {
        Some(s) => {
            if s.is_expired() {
                Ok(SessionStatus::Expired)
            } else {
                Ok(SessionStatus::Valid)
            }
        }
        None => Ok(SessionStatus::NotFound),
    }
}

/// 解析凭据，按优先级查找：keyring → 环境变量 → 交互式输入
///
/// `username_hint` 可由调用方预填（如 elective 保存在 config.toml 中的用户名），
/// 仅在 keyring/env 均无凭据时作为交互式输入的默认值。
pub fn resolve_credential(username_hint: Option<&str>) -> Result<Credential> {
    // 1. 尝试 OS keyring
    if let Some(cred) = try_keyring()? {
        println!(
            "{} 使用系统密钥链中的凭据 ({})",
            "[auth]".cyan(),
            cred.username
        );
        return Ok(cred);
    }

    // 2. 尝试环境变量
    if let Some(cred) = try_env_var()? {
        println!(
            "{} 使用环境变量中的凭据 ({})",
            "[auth]".cyan(),
            cred.username
        );
        return Ok(cred);
    }

    // 3. 交互式输入
    interactive_input(username_hint)
}

/// 尝试从 OS keyring 读取凭据
fn try_keyring() -> Result<Option<Credential>> {
    let kr = match keyring::Entry::new(KEYRING_SERVICE, "username") {
        Ok(e) => e,
        Err(_) => return Ok(None),
    };

    let username = match kr.get_password() {
        Ok(u) => u,
        Err(keyring::Error::NoEntry) | Err(keyring::Error::PlatformFailure(_)) => return Ok(None),
        Err(e) => {
            tracing::debug!("keyring 读取用户名失败: {e}");
            return Ok(None);
        }
    };

    let kr_pw = match keyring::Entry::new(KEYRING_SERVICE, "password") {
        Ok(e) => e,
        Err(_) => return Ok(None),
    };

    let password = match kr_pw.get_password() {
        Ok(p) => p,
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(e) => {
            tracing::debug!("keyring 读取密码失败: {e}");
            return Ok(None);
        }
    };

    Ok(Some(Credential {
        username,
        password,
        source: CredentialSource::Keyring,
    }))
}

/// 尝试从环境变量读取凭据
fn try_env_var() -> Result<Option<Credential>> {
    let username = std::env::var(ENV_USERNAME).ok();
    let password = std::env::var(ENV_PASSWORD).ok();

    match (username, password) {
        (Some(u), Some(p)) if !u.is_empty() && !p.is_empty() => Ok(Some(Credential {
            username: u,
            password: p,
            source: CredentialSource::EnvVar,
        })),
        _ => Ok(None),
    }
}

/// 交互式输入凭据
fn interactive_input(username_hint: Option<&str>) -> Result<Credential> {
    let username = match username_hint {
        Some(hint) if !hint.is_empty() => {
            println!("{} 使用已保存的用户名: {}", "[info]".cyan(), hint);
            hint.to_string()
        }
        _ => {
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

    Ok(Credential {
        username,
        password,
        source: CredentialSource::Interactive,
    })
}

// ─── Keyring 管理命令 ─────────────────────────────────────────

/// 将凭据保存到 OS keyring
pub fn keyring_store(username: &str, password: &str) -> Result<()> {
    let kr_user =
        keyring::Entry::new(KEYRING_SERVICE, "username").context("创建 keyring entry 失败")?;
    kr_user
        .set_password(username)
        .context("保存用户名到 keyring 失败")?;

    let kr_pw =
        keyring::Entry::new(KEYRING_SERVICE, "password").context("创建 keyring entry 失败")?;
    kr_pw
        .set_password(password)
        .context("保存密码到 keyring 失败")?;

    // 立即回读验证
    let verify = kr_user.get_password().context("keyring 回读验证失败")?;
    if verify != username {
        return Err(anyhow!("keyring 回读验证不一致"));
    }

    Ok(())
}

/// 从 OS keyring 删除凭据
pub fn keyring_clear() -> Result<()> {
    for key in ["username", "password"] {
        if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, key) {
            match entry.delete_credential() {
                Ok(()) => {}
                Err(keyring::Error::NoEntry) => {}
                Err(e) => {
                    return Err(anyhow!("删除 keyring 中的 {key} 失败: {e}"));
                }
            }
        }
    }
    Ok(())
}

/// 检查 keyring 中是否存储了凭据，返回 (是否存在, 错误详情)
pub fn keyring_has_credential() -> (bool, Option<String>) {
    let entry = match keyring::Entry::new(KEYRING_SERVICE, "username") {
        Ok(e) => e,
        Err(e) => return (false, Some(format!("创建 entry 失败: {e}"))),
    };
    match entry.get_password() {
        Ok(_) => (true, None),
        Err(keyring::Error::NoEntry) => (false, None),
        Err(e) => (false, Some(format!("读取失败: {e}"))),
    }
}

// ─── 短信验证码解析 ──────────────────────────────────────────

/// 获取短信验证码：环境变量 `PKU_SMS_CODE` → 交互式输入
///
/// AI Agent 可以先询问用户获取验证码，再通过环境变量传入，
/// 避免 Agent 需要控制 stdin。
pub fn resolve_sms_code(prompt: &str) -> Result<String> {
    // 1. 检查环境变量
    if let Ok(code) = std::env::var(ENV_SMS_CODE) {
        let code = code.trim().to_string();
        if !code.is_empty() {
            println!("{} 使用环境变量中的短信验证码", "[auth]".cyan());
            return Ok(code);
        }
    }

    // 2. 交互式输入
    print!("{prompt}");
    io::stdout().flush()?;
    let mut code = String::new();
    io::stdin().read_line(&mut code)?;
    let code = code.trim().to_string();
    if code.is_empty() {
        return Err(anyhow!("验证码不能为空"));
    }
    Ok(code)
}

/// 获取是/否确认：环境变量 `PKU_SMS_CODE` 存在时自动确认，否则交互式询问
///
/// 当 Agent 通过环境变量传入验证码时，自动跳过"是否发送"的确认步骤。
pub fn confirm_send_sms(prompt: &str) -> Result<bool> {
    // 如果环境变量中已有验证码，自动确认发送
    if std::env::var(ENV_SMS_CODE).is_ok() {
        println!("{} 自动确认发送短信验证码", "[auth]".cyan());
        return Ok(true);
    }

    print!("{prompt}");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(!input.trim().eq_ignore_ascii_case("n"))
}
