//! 北大网盘（disk.pku.edu.cn，爱数 AnyShare 7）CLI
//!
//! - IAAA 登录（appID=anyshare）→ 网盘 OAuth2，访问令牌自动续期
//! - 列出全部文档库（个人 / 其他 / 部门 / 自定义 / 知识库）并逐级浏览
//! - 下载文件或整个文件夹（保持目录结构，已下载的跳过）

mod api;
mod client;
pub mod commands;
mod display;
pub mod login;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "disk", about = "北大网盘 CLI 客户端", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// 登录北大网盘（通过 IAAA 统一身份认证）
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

    /// 列出文档库，或某个文件夹里的内容
    Ls {
        /// 路径，如「个人/课件」「部门文档库/xxx」；第一段是文档库（名字、序号、「个人」或类型名）。
        /// 不填则列出全部文档库
        path: Option<String>,
    },
    /// 下载文件或整个文件夹（保持目录结构，大小相同的已有文件会跳过）
    #[command(alias = "dl")]
    Download {
        /// 路径，写法同 ls
        path: String,
        /// 输出目录（默认当前目录）
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

pub async fn run() -> Result<()> {
    dispatch(Cli::parse().command).await
}

pub async fn run_from<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    dispatch(Cli::try_parse_from(args)?.command).await
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
                let mode = if open {
                    pkuinfo_common::qr::QrDisplayMode::Open
                } else {
                    pkuinfo_common::qr::QrDisplayMode::Terminal
                };
                login::login_with_qrcode(mode).await?;
            }
        }
        Commands::Status => login::status()?,
        Commands::Logout => login::logout()?,
        Commands::Ls { path } => commands::cmd_ls(path.as_deref()).await?,
        Commands::Download { path, output } => {
            commands::cmd_download(&path, output.as_deref()).await?
        }
    }
    Ok(())
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
