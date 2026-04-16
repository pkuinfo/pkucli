mod api;
mod client;
pub mod commands;
mod config;
mod display;
pub mod login;

/// 暴露 client::build 供其他 crate（如 claspider）复用
pub fn client_build(
    cookie_store: std::sync::Arc<reqwest_cookie_store::CookieStoreMutex>,
) -> anyhow::Result<reqwest::Client> {
    client::build(cookie_store)
}

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "elective", about = "北大选课网 CLI 客户端", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// 登录选课网（通过 IAAA 统一身份认证）
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
        /// 双学位选择
        #[arg(short, long, value_enum)]
        dual: Option<login::DualDegree>,
    },
    /// 查看当前登录状态
    Status,
    /// 退出登录
    Logout,

    /// 查看选课结果
    Show,

    /// 浏览补退选课程列表
    #[command(alias = "ls")]
    List {
        /// 页码（从 1 开始，默认第 1 页）
        #[arg(short, long)]
        page: Option<usize>,
    },

    /// 添加自动选课目标（交互式从补退选列表中选择）
    Set,

    /// 移除自动选课目标
    Unset,

    /// 配置验证码识别后端
    #[command(alias = "captcha")]
    ConfigCaptcha {
        /// 后端类型: manual / utool / ttshitu / yunma
        backend: String,
    },

    /// 启动自动选课循环（持续监控并尝试选课）
    Launch {
        /// 检查间隔（秒，默认 15）
        #[arg(short = 't', long, default_value = "15")]
        interval: u64,
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
        secret: String,
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
        // ── Auth ──
        Commands::Login {
            password,
            username,
            open,
            dual,
        } => {
            if password {
                login::login_with_password(username.as_deref(), dual.as_ref()).await?;
            } else {
                let qr_mode = if open {
                    pkuinfo_common::qr::QrDisplayMode::Open
                } else {
                    pkuinfo_common::qr::QrDisplayMode::Terminal
                };
                login::login_with_qrcode(qr_mode, dual.as_ref()).await?;
            }
        }
        Commands::Status => login::status()?,
        Commands::Logout => login::logout()?,

        // ── Browse ──
        Commands::Show => commands::cmd_show().await?,
        Commands::List { page } => {
            // CLI 用 1-indexed，内部 0-indexed
            let page = page.map(|p| p.saturating_sub(1));
            commands::cmd_list(page).await?;
        }

        // ── Auto-elect config ──
        Commands::Set => commands::cmd_set().await?,
        Commands::Unset => commands::cmd_unset()?,
        Commands::ConfigCaptcha { backend } => commands::cmd_config_captcha(&backend)?,

        // ── Launch ──
        Commands::Launch { interval } => commands::cmd_launch(interval).await?,

        // ── OTP ──
        Commands::Otp { action } => {
            let store = pkuinfo_common::session::Store::new("elective")?;
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
