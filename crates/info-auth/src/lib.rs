//! PKU 统一凭据管理 CLI
//!
//! 该工具用于安全地管理 IAAA 认证凭据。密码通过交互式输入，
//! 存储在操作系统密钥链中，**永远不会以明文形式出现在命令行参数中**。
//!
//! AI Agent 无法通过此工具获取密码 ---- 所有密码输入都在 CLI 进程内完成。
//!
//! ## 典型使用流程
//!
//! ```bash
//! # 用户手动运行一次，交互式输入密码存入系统密钥链
//! info-auth store
//!
//! # 之后 AI Agent 只需调用各 CLI 的 login 命令即可自动认证
//! treehole login -p
//! course login -p
//!
//! # 查看凭据状态（不显示密码）
//! info-auth status
//!
//! # 查看所有服务的会话状态
//! info-auth check
//!
//! # 清除密钥链中的凭据
//! info-auth clear
//! ```

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use pkuinfo_common::credential;
use std::io::{self, Write};

#[derive(Parser)]
#[command(name = "info-auth")]
#[command(about = "PKU 统一凭据管理 -- 安全存储 IAAA 认证凭据到系统密钥链")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// 交互式输入并存储凭据到系统密钥链
    #[command(alias = "save")]
    Store,

    /// 查看凭据存储状态（不显示密码）
    Status,

    /// 查看所有服务的会话状态
    Check,

    /// 清除系统密钥链中的凭据
    Clear,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    dispatch(cli.command)
}

pub fn run_from<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::try_parse_from(args)?;
    dispatch(cli.command)
}

pub fn dispatch(command: Commands) -> Result<()> {
    match command {
        Commands::Store => cmd_store()?,
        Commands::Status => cmd_status()?,
        Commands::Check => cmd_check()?,
        Commands::Clear => cmd_clear()?,
    }

    Ok(())
}

fn cmd_store() -> Result<()> {
    println!("{}", "将 PKU IAAA 凭据安全存储到系统密钥链".bold());
    println!(
        "{}",
        "密码将由操作系统加密保护，不会以明文保存到磁盘。".dimmed()
    );
    println!();

    // 交互式输入用户名
    print!("学号/职工号: ");
    io::stdout().flush()?;
    let mut username = String::new();
    io::stdin().read_line(&mut username)?;
    let username = username.trim();

    if username.is_empty() {
        return Err(anyhow!("用户名不能为空"));
    }

    // 交互式输入密码（不回显）
    print!("密码: ");
    io::stdout().flush()?;
    let password = rpassword::read_password()?;
    if password.is_empty() {
        return Err(anyhow!("密码不能为空"));
    }

    // 确认密码
    print!("确认密码: ");
    io::stdout().flush()?;
    let confirm = rpassword::read_password()?;
    if password != confirm {
        return Err(anyhow!("两次输入的密码不一致"));
    }

    // 存入 keyring
    credential::keyring_store(username, &password)?;

    println!();
    println!("{} 凭据已安全存储到系统密钥链", "✓".green().bold());
    println!("  用户名: {}", username);
    println!("  密码:   {}", "*".repeat(password.len()).dimmed());
    println!();
    println!(
        "{}",
        "现在可以使用以下命令自动登录（无需再次输入密码）：".dimmed()
    );
    println!("  treehole login -p");
    println!("  course login -p");
    println!("  campuscard login -p");
    println!("  elective login -p");

    Ok(())
}

fn cmd_status() -> Result<()> {
    let (has_cred, err_detail) = credential::keyring_has_credential();
    if has_cred {
        println!("{} 系统密钥链中已存储 PKU 凭据", "●".green());

        // 只显示用户名，不显示密码
        if let Ok(Some(cred)) = try_read_username() {
            println!("  用户名: {}", cred);
        }
        println!("  密码:   {}", "(已加密存储)".dimmed());
    } else {
        println!(
            "{} 系统密钥链中未存储凭据。运行 `info-auth store` 开始。",
            "○".red()
        );
        if let Some(detail) = err_detail {
            println!("  {}: {}", "诊断".dimmed(), detail);
        }
    }

    // 检查环境变量
    let has_env_user = std::env::var("PKU_USERNAME").is_ok();
    let has_env_pass = std::env::var("PKU_PASSWORD").is_ok();
    if has_env_user && has_env_pass {
        println!("{} 环境变量 PKU_USERNAME/PKU_PASSWORD 已设置", "●".green());
    } else if has_env_user || has_env_pass {
        println!(
            "{} 环境变量设置不完整（需要同时设置 PKU_USERNAME 和 PKU_PASSWORD）",
            "●".yellow()
        );
    }

    Ok(())
}

fn cmd_check() -> Result<()> {
    println!("{}", "各服务会话状态：".bold());

    let services = [
        ("treehole", "树洞"),
        ("course", "教学网"),
        ("campuscard", "校园卡"),
        ("elective", "选课网"),
    ];

    for (name, label) in services {
        let status = credential::check_session(name)?;
        match status {
            credential::SessionStatus::Valid => {
                println!("  {} {} — {}", "●".green(), label, "会话有效".green());
            }
            credential::SessionStatus::Expired => {
                println!(
                    "  {} {} — {}",
                    "●".yellow(),
                    label,
                    "会话已过期，需重新登录".yellow()
                );
            }
            credential::SessionStatus::NotFound => {
                println!("  {} {} — {}", "○".red(), label, "未登录".red());
            }
        }
    }

    Ok(())
}

fn cmd_clear() -> Result<()> {
    credential::keyring_clear()?;
    println!("{} 已清除系统密钥链中的 PKU 凭据", "✓".green());
    Ok(())
}

/// 仅读取 keyring 中的用户名（用于 status 展示）
fn try_read_username() -> Result<Option<String>> {
    let entry = keyring::Entry::new("info-pku", "username")?;
    match entry.get_password() {
        Ok(u) => Ok(Some(u)),
        Err(_) => Ok(None),
    }
}
