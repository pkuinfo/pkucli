//! 北京大学校内信息门户（portal.pku.edu.cn）相关 CLI
//!
//! 当前实现：
//! - 空闲教室：`portal free-classroom 一教 --day today`
//! - 校历：`portal calendar [--year 2025-2026]`
//! - 网费：`portal netfee status` / `portal netfee recharge <amount>`
//!   / `portal netfee watch --threshold 10`

pub mod calendar;
mod client;
pub mod freeclassroom;
pub mod netfee;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use pkuinfo_common::captcha::CaptchaConfig;

pub const APP_NAME: &str = "portal";

#[derive(Parser)]
#[command(name = "portal", about = "北京大学校内信息门户 CLI", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// 查询空闲教室
    #[command(alias = "fc")]
    FreeClassroom {
        /// 教学楼：一教 / 二教 / 三教 / 四教 / 理教 / 文史 / 哲学 / 地学 / 国关 / 政管
        building: String,
        /// 日期：today / tomorrow / day-after
        #[arg(short, long, default_value = "today")]
        day: String,
    },

    /// 显示校历
    #[command(alias = "cal")]
    Calendar {
        /// 只显示指定学年（如 2025-2026）
        #[arg(short, long)]
        year: Option<String>,
    },

    /// 网费相关（查询 / 充值 / 低余额监测）
    Netfee {
        #[command(subcommand)]
        action: NetfeeAction,
    },
}

#[derive(Subcommand)]
pub enum NetfeeAction {
    /// 查询账户状态（余额 / 本月使用 / 在线会话）
    Status {
        /// 学号/职工号（可选；留空走 keyring → env → 交互）
        #[arg(short, long)]
        username: Option<String>,
    },
    /// 充值
    Recharge {
        /// 充值金额（元），范围 (0, 500]
        amount: f64,
        #[arg(short, long)]
        username: Option<String>,
        /// 验证码后端：manual / utool / ttshitu / yunma（默认 utool 免费）
        #[arg(long, default_value = "utool")]
        captcha: String,
        /// 支付方式：wechat / alipay（默认 wechat）
        #[arg(short, long, default_value = "wechat")]
        method: String,
    },
    /// 低余额监测：余额低于阈值时返回退出码 2（便于脚本 + cron 报警）
    Watch {
        /// 阈值（元）
        #[arg(short, long, default_value_t = 10.0)]
        threshold: f64,
        #[arg(short, long)]
        username: Option<String>,
    },
}

pub async fn run() -> Result<()> {
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
        Commands::FreeClassroom { building, day } => {
            let day = freeclassroom::Day::parse(&day)?;
            let rows = freeclassroom::query(&building, day).await?;
            freeclassroom::render(&building, day, &rows);
        }
        Commands::Calendar { year } => {
            let cals = calendar::fetch().await?;
            calendar::render(&cals, year.as_deref());
        }
        Commands::Netfee { action } => match action {
            NetfeeAction::Status { username } => {
                let s = netfee::query(username.as_deref()).await?;
                netfee::render_status(&s);
            }
            NetfeeAction::Recharge {
                amount,
                username,
                captcha,
                method,
            } => {
                let cfg = parse_captcha_backend(&captcha)?;
                let method = netfee::PayMethod::parse(&method)?;
                let store = pkuinfo_common::session::Store::new(APP_NAME)
                    .context("创建 portal config 目录失败")?;
                let result = netfee::recharge(
                    username.as_deref(),
                    amount,
                    method,
                    &cfg,
                    store.config_dir(),
                )
                .await?;
                netfee::print_qr_terminal(&result)?;
            }
            NetfeeAction::Watch {
                threshold,
                username,
            } => {
                let s = netfee::query(username.as_deref()).await?;
                netfee::render_status(&s);
                if netfee::is_low(&s, threshold) {
                    eprintln!();
                    eprintln!("{} 余额低于阈值 {threshold} 元", "[!]".red().bold());
                    std::process::exit(2);
                }
            }
        },
    }
    Ok(())
}

fn parse_captcha_backend(s: &str) -> Result<CaptchaConfig> {
    Ok(match s {
        "manual" => CaptchaConfig::Manual,
        "utool" => CaptchaConfig::Utool,
        "ttshitu" => {
            return Err(anyhow::anyhow!(
                "ttshitu 需要 username/password，请在 elective config-captcha 中配置后使用"
            ))
        }
        "yunma" => {
            return Err(anyhow::anyhow!(
                "yunma 需要 token，请在 elective config-captcha 中配置后使用"
            ))
        }
        other => return Err(anyhow::anyhow!("未知验证码后端: {other}")),
    })
}
