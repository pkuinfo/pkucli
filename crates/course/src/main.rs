mod api;
mod client;
mod commands;
mod display;
mod login;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "course", about = "北大教学网 CLI 客户端", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 登录教学网（通过 IAAA 统一身份认证）
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

    /// 查看课程列表
    #[command(alias = "ls")]
    Courses {
        /// 显示所有课程（含往期），默认只显示当前学期
        #[arg(short, long)]
        all: bool,
    },

    /// 查看课程侧边栏入口
    Info {
        /// 课程 ID 或编号（从 courses 列表中获取）
        course: String,
    },

    /// 查看内容列表
    Content {
        /// 课程 ID 或编号
        course: String,
        /// content_id
        content_id: String,
    },

    /// 查看作业详情
    Assignment {
        /// 课程 ID 或编号
        course: String,
        /// content_id
        content_id: String,
    },

    /// 下载文件
    Download {
        /// 文件 URL
        url: String,
        /// 输出目录（默认当前目录）
        #[arg(short, long)]
        output: Option<String>,
    },

    /// 交互式浏览课程内容
    Browse {
        /// 课程 ID 或编号（不提供则交互选择）
        course: Option<String>,
    },
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

        // ── Browse ──
        Commands::Courses { all } => commands::cmd_courses(all).await?,
        Commands::Info { course } => commands::cmd_info(&course).await?,
        Commands::Content { course, content_id } => {
            commands::cmd_content(&course, &content_id).await?
        }
        Commands::Assignment { course, content_id } => {
            commands::cmd_assignment(&course, &content_id).await?
        }
        Commands::Download { url, output } => {
            commands::cmd_download(&url, output.as_deref()).await?
        }
        Commands::Browse { course } => commands::cmd_browse(course.as_deref()).await?,
    }
    Ok(())
}
