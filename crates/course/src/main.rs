mod api;
mod client;
mod commands;
mod display;
mod login;
mod multipart;

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

    /// 查看所有作业（跨课程汇总，按截止时间排序）
    #[command(alias = "als")]
    Assignments {
        /// 显示所有作业（含已完成），默认只显示未完成
        #[arg(short, long)]
        all: bool,
        /// 包含往期学期的作业
        #[arg(long)]
        all_term: bool,
    },

    /// 下载作业附件（按作业哈希 ID 或交互选择）
    #[command(alias = "adl")]
    AssignmentDownload {
        /// 作业哈希 ID（从 assignments 列表中获取，不提供则交互选择）
        id: Option<String>,
        /// 输出目录（默认当前目录）
        #[arg(short, long)]
        output: Option<String>,
        /// 包含往期学期的作业
        #[arg(long)]
        all_term: bool,
    },

    /// 下载文件
    Download {
        /// 文件 URL
        url: String,
        /// 输出目录（默认当前目录）
        #[arg(short, long)]
        output: Option<String>,
    },

    /// 提交作业（指定课程+内容 ID，或交互选择）
    Submit {
        /// 课程 ID 或编号
        course: Option<String>,
        /// content_id
        content_id: Option<String>,
        /// 提交文件路径（不提供则交互选择当前目录文件）
        file: Option<String>,
    },

    /// 查看课程回放列表
    #[command(alias = "vls")]
    Videos {
        /// 课程 ID 或编号（不提供则列出所有课程的回放）
        course: Option<String>,
        /// 包含往期学期
        #[arg(long)]
        all_term: bool,
    },

    /// 下载课程回放视频（按视频序号或哈希 ID）
    #[command(alias = "vdl")]
    VideoDownload {
        /// 视频序号或哈希 ID（从 videos 列表中获取）
        id: String,
        /// 课程 ID 或编号（不提供则在所有课程中搜索）
        #[arg(short, long)]
        course: Option<String>,
        /// 输出目录（默认当前目录）
        #[arg(short, long)]
        output: Option<String>,
        /// 包含往期学期
        #[arg(long)]
        all_term: bool,
    },

    /// 交互式浏览课程内容
    Browse {
        /// 课程 ID 或编号（不提供则交互选择）
        course: Option<String>,
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

        // ── Browse ──
        Commands::Courses { all } => commands::cmd_courses(all).await?,
        Commands::Info { course } => commands::cmd_info(&course).await?,
        Commands::Content { course, content_id } => {
            commands::cmd_content(&course, &content_id).await?
        }
        Commands::Assignment { course, content_id } => {
            commands::cmd_assignment(&course, &content_id).await?
        }
        Commands::Assignments { all, all_term } => {
            commands::cmd_assignments(all, all_term).await?
        }
        Commands::AssignmentDownload {
            id,
            output,
            all_term,
        } => {
            commands::cmd_assignment_download(id.as_deref(), output.as_deref(), all_term).await?
        }
        Commands::Download { url, output } => {
            commands::cmd_download(&url, output.as_deref()).await?
        }
        Commands::Submit {
            course,
            content_id,
            file,
        } => {
            commands::cmd_submit(course.as_deref(), content_id.as_deref(), file.as_deref()).await?
        }
        Commands::Videos { course, all_term } => {
            commands::cmd_videos(course.as_deref(), all_term).await?
        }
        Commands::VideoDownload {
            id,
            course,
            output,
            all_term,
        } => {
            commands::cmd_video_download(course.as_deref(), &id, output.as_deref(), all_term)
                .await?
        }
        Commands::Browse { course } => commands::cmd_browse(course.as_deref()).await?,

        // ── OTP ──
        Commands::Otp { action } => {
            let store = info_common::session::Store::new("course")?;
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
