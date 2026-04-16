//! 北大财务综合信息门户 CLI 客户端
//!
//! 目前实现了：
//! - IAAA 登录（app_id = `IIPF`）
//! - 账务查询 → 收入查询 → 个人酬金查询

pub mod api;
mod client;
pub mod commands;
pub mod context;
mod display;
pub mod encrypt;
pub mod login;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cwfw", about = "北大财务综合信息门户 CLI 客户端", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// 登录财务门户（通过 IAAA 统一身份认证）
    Login {
        /// 使用用户名密码登录（默认为扫码登录）
        #[arg(short, long)]
        password: bool,
        /// 学号/职工号（仅密码登录时需要）
        #[arg(short, long)]
        username: Option<String>,
        /// 用系统图片查看器打开二维码（默认终端渲染）
        #[arg(long)]
        open: bool,
    },
    /// 查看当前登录状态
    Status,
    /// 退出登录
    Logout,

    /// 个人酬金查询（默认当前年份、1 月到当前月）
    #[command(alias = "reward")]
    Cj {
        /// 年份，默认当前年
        #[arg(short, long)]
        year: Option<u32>,
        /// 起始月份 (1-12)，默认 1
        #[arg(short, long)]
        from: Option<u32>,
        /// 结束月份 (1-12)，默认当前月
        #[arg(short, long)]
        to: Option<u32>,
    },

    /// 手机令牌 (OTP) 管理
    Otp {
        #[command(subcommand)]
        action: OtpAction,
    },
}

#[derive(Subcommand)]
pub enum OtpAction {
    /// 绑定手机令牌（默认交互式；支持 --send / --verify 两阶段绑定）
    Bind {
        /// 学号/职工号
        #[arg(short, long)]
        username: Option<String>,
        /// 只发送短信验证码并保存会话，不等待输入（供 AI Agent 使用）
        #[arg(long, conflicts_with = "verify")]
        send: bool,
        /// 用已保存的会话和指定短信验证码完成绑定
        #[arg(long, value_name = "CODE", conflicts_with = "send")]
        verify: Option<String>,
    },
    /// 手动设置 TOTP secret
    Set {
        /// Base32 编码的 TOTP secret
        secret: String,
        /// 学号/职工号
        #[arg(short, long)]
        username: Option<String>,
    },
    /// 查看当前 OTP 码
    Show,
    /// 清除已保存的 OTP 配置
    Clear,
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
}

pub async fn run() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();
    dispatch(cli.command).await
}

pub async fn run_from<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::try_parse_from(args)?;
    dispatch(cli.command).await
}

pub async fn dispatch(command: Commands) -> Result<()> {
    match command {
        Commands::Login {
            password,
            username,
            open,
        } => {
            if password {
                login::login_with_password(username.as_deref()).await?;
            } else {
                let qr_mode = if open {
                    pkuinfo_common::qr::QrDisplayMode::Open
                } else {
                    pkuinfo_common::qr::QrDisplayMode::Terminal
                };
                login::login_with_qrcode(qr_mode).await?;
            }
        }
        Commands::Status => login::status()?,
        Commands::Logout => login::logout()?,

        Commands::Cj { year, from, to } => commands::cmd_reward(year, from, to).await?,

        Commands::Otp { action } => {
            let store = pkuinfo_common::session::Store::new(login::APP_NAME)?;
            handle_otp(action, store.config_dir()).await?;
        }
    }
    Ok(())
}

async fn handle_otp(action: OtpAction, config_dir: &std::path::Path) -> anyhow::Result<()> {
    use colored::Colorize;
    match action {
        OtpAction::Bind {
            username,
            send,
            verify,
        } => {
            if send {
                pkuinfo_common::otp::bind_otp_send_sms(config_dir, username.as_deref()).await?;
            } else if let Some(code) = verify {
                pkuinfo_common::otp::bind_otp_verify(config_dir, &code).await?;
            } else {
                pkuinfo_common::otp::bind_otp_interactive(config_dir, username.as_deref()).await?;
            }
        }
        OtpAction::Set { secret, username } => {
            let uid = username.unwrap_or_default();
            pkuinfo_common::otp::set_otp_secret(config_dir, &secret, &uid)?;
        }
        OtpAction::Show => match pkuinfo_common::otp::get_current_otp(config_dir)? {
            Some(code) => {
                let config = pkuinfo_common::otp::load_otp_config(config_dir)?
                    .ok_or_else(|| anyhow::anyhow!("OTP 配置文件缺失，请先运行 `otp bind`"))?;
                println!(
                    "{} {} ({})",
                    "OTP:".green().bold(),
                    code.bold(),
                    config.user_id
                );
            }
            None => {
                println!(
                    "{} 未配置 OTP。使用 `otp bind` 绑定或 `otp set <SECRET>` 手动设置",
                    "○".red()
                );
            }
        },
        OtpAction::Clear => {
            pkuinfo_common::otp::clear_otp_config(config_dir)?;
            println!("{} OTP 配置已清除", "✓".green());
        }
    }
    Ok(())
}
