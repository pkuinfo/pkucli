mod api;
mod client;
mod commands;
mod display;
mod login;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "campuscard", about = "北大校园卡 CLI 客户端", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 登录校园卡（通过 IAAA 统一身份认证）
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

    /// 查看校园卡信息和余额
    Info,

    /// 显示付款码（二维码）
    Pay,

    /// 充值校园卡
    Recharge {
        /// 充值金额（元），不提供则交互选择
        #[arg(short, long)]
        amount: Option<f64>,
    },

    /// 查看交易记录
    #[command(alias = "ls")]
    Bills {
        /// 页码（默认 1）
        #[arg(short, long, default_value = "1")]
        page: usize,
        /// 每页条数（默认 10）
        #[arg(short = 'n', long, default_value = "10")]
        size: usize,
        /// 按月筛选，格式 YYYY-MM
        #[arg(short, long)]
        month: Option<String>,
    },

    /// 查看消费统计
    Stats {
        /// 月份，格式 YYYY-MM（默认当月）
        #[arg(short, long)]
        month: Option<String>,
    },

    /// 手机令牌 (OTP) 管理
    Otp {
        #[command(subcommand)]
        action: OtpAction,
    },
}

#[derive(Subcommand)]
enum OtpAction {
    /// 绑定手机令牌（交互式，需要短信验证）
    Bind {
        /// 学号/职工号
        #[arg(short, long)]
        username: Option<String>,
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn".into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        // ── Auth ──
        Commands::Login {
            password,
            username,
            open,
        } => {
            if password {
                login::login_with_password(username.as_deref()).await?;
            } else {
                let qr_mode = if open {
                    info_common::qr::QrDisplayMode::Open
                } else {
                    info_common::qr::QrDisplayMode::Terminal
                };
                login::login_with_qrcode(qr_mode).await?;
            }
        }
        Commands::Status => login::status()?,
        Commands::Logout => login::logout()?,

        // ── Card ──
        Commands::Info => commands::cmd_info().await?,
        Commands::Pay => commands::cmd_pay().await?,
        Commands::Recharge { amount } => commands::cmd_recharge(amount).await?,
        Commands::Bills { page, size, month } => {
            commands::cmd_bills(Some(page), Some(size), month.as_deref()).await?;
        }
        Commands::Stats { month } => commands::cmd_stats(month.as_deref()).await?,

        // ── OTP ──
        Commands::Otp { action } => {
            let store = info_common::session::Store::new("campuscard")?;
            handle_otp(action, store.config_dir()).await?;
        }
    }
    Ok(())
}

async fn handle_otp(action: OtpAction, config_dir: &std::path::Path) -> anyhow::Result<()> {
    use colored::Colorize;
    match action {
        OtpAction::Bind { username } => {
            info_common::otp::bind_otp_interactive(config_dir, username.as_deref()).await?;
        }
        OtpAction::Set { secret, username } => {
            let uid = username.unwrap_or_default();
            info_common::otp::set_otp_secret(config_dir, &secret, &uid)?;
        }
        OtpAction::Show => match info_common::otp::get_current_otp(config_dir)? {
            Some(code) => {
                let config = info_common::otp::load_otp_config(config_dir)?
                    .expect("OTP 配置存在");
                println!("{} {} ({})", "OTP:".green().bold(), code.bold(), config.user_id);
            }
            None => {
                println!(
                    "{} 未配置 OTP。使用 `otp bind` 绑定或 `otp set <SECRET>` 手动设置",
                    "○".red()
                );
            }
        },
        OtpAction::Clear => {
            info_common::otp::clear_otp_config(config_dir)?;
            println!("{} OTP 配置已清除", "✓".green());
        }
    }
    Ok(())
}
