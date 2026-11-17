mod api;
mod client;
pub mod commands;
mod display;
pub mod login;
mod multipart;
mod ta;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "course", about = "北大教学网 CLI 客户端", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
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

    /// 查看课程公告
    #[command(alias = "ann")]
    Announcements {
        /// 课程 ID 或编号（不提供则汇总当前学期所有课程的公告）
        course: Option<String>,
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

    /// 助教工具：发内容、建作业、下载提交、登分（写操作需加 --yes）
    Ta {
        #[command(subcommand)]
        action: TaAction,
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

/// 助教子命令（写操作默认只预览，加 --yes 才写入）
#[derive(Subcommand)]
pub enum TaAction {
    /// 列出内容区或文件夹里的条目（含对学生隐藏的）
    Ls {
        /// 课程 ID、编号或名称
        course: String,
        /// 文件夹 ID 或标题（如「教学内容」），不填列出顶层内容区
        folder: Option<String>,
    },
    /// 查看成绩中心的列
    Columns {
        /// 课程 ID、编号或名称
        course: String,
    },
    /// 查看学生名单（已排除预览账号）
    Roster {
        /// 课程 ID、编号或名称
        course: String,
    },
    /// 查看某次作业的提交和评分情况
    Attempts {
        /// 课程 ID、编号或名称
        course: String,
        /// 成绩列 ID 或名称（如「第三章作业提交」）
        column: String,
        /// 只显示待评分的
        #[arg(long)]
        needs_grading: bool,
    },
    /// 下载某次作业的全部提交，并生成成绩模板 CSV
    Download {
        /// 课程 ID、编号或名称
        course: String,
        /// 成绩列 ID 或名称
        column: String,
        /// 输出目录（默认以作业名命名）
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// 只下载待评分的
        #[arg(long)]
        needs_grading: bool,
    },
    /// 在文件夹里发布一个文件条目（教学内容、作业题目等）
    Post {
        /// 课程 ID、编号或名称
        course: String,
        /// 目标文件夹 ID 或标题
        folder: String,
        /// 要上传的文件
        file: PathBuf,
        /// 条目标题（默认用文件名）
        #[arg(short, long)]
        title: Option<String>,
        /// 条目说明
        #[arg(short, long)]
        desc: Option<String>,
        /// 先对学生隐藏
        #[arg(long)]
        hidden: bool,
        /// 确认写入（不加只预览）
        #[arg(short, long)]
        yes: bool,
    },
    /// 新建作业（带提交入口和成绩列）
    Assign {
        /// 课程 ID、编号或名称
        course: String,
        /// 目标文件夹 ID 或标题（如「课程作业」）
        folder: String,
        /// 作业标题
        #[arg(short, long)]
        title: String,
        /// 截止时间，如 "2026-11-01 23:59"；只写日期则为当天 23:59
        #[arg(long)]
        due: String,
        /// 满分
        #[arg(long, default_value_t = 100.0)]
        points: f64,
        /// 允许提交次数，0 为不限
        #[arg(long, default_value_t = 1)]
        attempts: u32,
        /// 题目附件，可重复
        #[arg(short, long)]
        file: Vec<PathBuf>,
        /// 作业说明
        #[arg(short, long)]
        instructions: Option<String>,
        /// 先对学生隐藏
        #[arg(long)]
        hidden: bool,
        /// 确认写入（不加只预览）
        #[arg(short, long)]
        yes: bool,
    },
    /// 给一名学生的作业登分，可附评语和反馈文件
    Grade {
        /// 课程 ID、编号或名称
        course: String,
        /// 成绩列 ID 或名称
        column: String,
        /// 学号或姓名
        student: String,
        /// 分数
        #[arg(short, long)]
        score: String,
        /// 评语（纯文本，换行分段）
        #[arg(long)]
        feedback: Option<String>,
        /// 反馈文件，可重复
        #[arg(short, long)]
        file: Vec<PathBuf>,
        /// 确认写入（不加只预览）
        #[arg(short, long)]
        yes: bool,
    },
    /// 按 CSV 批量登分（列：学号,姓名,分数,评语，即 download 生成的模板）
    GradeBatch {
        /// 课程 ID、编号或名称
        course: String,
        /// 成绩列 ID 或名称
        column: String,
        /// 成绩 CSV；分数留空的行跳过
        csv: PathBuf,
        /// 反馈文件目录：其中文件名以学号开头的文件会作为该生的反馈文件上传
        #[arg(long)]
        files_dir: Option<PathBuf>,
        /// 确认写入（不加只预览）
        #[arg(short, long)]
        yes: bool,
    },
    /// 查看课堂实录的录像及发布状态
    Videos {
        /// 课程 ID、编号或名称
        course: String,
    },
    /// 发布课堂实录录像（不指定则发布全部未发布的）
    Publish {
        /// 课程 ID、编号或名称
        course: String,
        /// 录像序号、录像 ID 或标题片段（如 10-21），可多个
        videos: Vec<String>,
        /// 确认写入（不加只预览）
        #[arg(short, long)]
        yes: bool,
    },
    /// 撤销课堂实录录像的发布（学生将看不到）
    Unpublish {
        /// 课程 ID、编号或名称
        course: String,
        /// 录像序号、录像 ID 或标题片段，可多个
        #[arg(required = true)]
        videos: Vec<String>,
        /// 确认写入（不加只预览）
        #[arg(short, long)]
        yes: bool,
    },
    /// 删除内容条目（作业已有学生提交、文件夹非空时拒绝）
    Rm {
        /// 课程 ID、编号或名称
        course: String,
        /// 内容 ID
        content_id: String,
        /// 确认删除（不加只预览）
        #[arg(short, long)]
        yes: bool,
    },
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

        // ── Browse ──
        Commands::Courses { all } => commands::cmd_courses(all).await?,
        Commands::Info { course } => commands::cmd_info(&course).await?,
        Commands::Content { course, content_id } => {
            commands::cmd_content(&course, &content_id).await?
        }
        Commands::Assignment { course, content_id } => {
            commands::cmd_assignment(&course, &content_id).await?
        }
        Commands::Assignments { all, all_term } => commands::cmd_assignments(all, all_term).await?,
        Commands::AssignmentDownload {
            id,
            output,
            all_term,
        } => commands::cmd_assignment_download(id.as_deref(), output.as_deref(), all_term).await?,
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
        Commands::Announcements { course } => {
            commands::cmd_announcements(course.as_deref()).await?
        }
        Commands::Browse { course } => commands::cmd_browse(course.as_deref()).await?,

        // ── OTP ──
        Commands::Otp { action } => {
            let store = pkuinfo_common::session::Store::new("course")?;
            handle_otp(action, store.config_dir()).await?;
        }

        // ── TA ──
        Commands::Ta { action } => commands::cmd_ta(action).await?,
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

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
