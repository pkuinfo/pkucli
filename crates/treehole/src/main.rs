mod api;
mod client;
mod colorize;
mod commands;
mod display;
mod login;
mod verify;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "treehole", about = "北大树洞 CLI 客户端", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 登录北大树洞（通过 IAAA 统一身份认证）
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

    /// 浏览帖子列表
    #[command(alias = "ls")]
    List {
        /// 信息流类型：latest（默认）/ follow
        #[arg(default_value = "latest")]
        feed: String,
        /// 页码
        #[arg(short, long, default_value = "1")]
        page: u32,
        /// 每页条数
        #[arg(short = 'n', long, default_value = "10")]
        limit: u32,
    },
    /// 查看帖子详情及评论
    Show {
        /// 帖子 PID
        pid: i64,
    },
    /// 搜索帖子
    Search {
        /// 搜索关键词或 #PID
        keyword: String,
        /// 页码
        #[arg(short, long, default_value = "1")]
        page: u32,
        /// 每页条数
        #[arg(short = 'n', long, default_value = "10")]
        limit: u32,
    },

    /// 发布新树洞
    Post {
        /// 帖子内容（不提供则进入交互式输入）
        #[arg(short, long)]
        text: Option<String>,
        /// 标签 ID（逗号分隔）
        #[arg(long)]
        tag: Option<String>,
        /// 使用昵称发帖（默认匿名）
        #[arg(long)]
        named: bool,
        /// 折叠显示
        #[arg(long)]
        fold: bool,
        /// 悬赏树叶数量
        #[arg(long)]
        reward: Option<i64>,
        /// 图片路径（可多次指定，如 --image a.jpg --image b.png）
        #[arg(short, long)]
        image: Vec<std::path::PathBuf>,
    },
    /// 回复帖子
    Reply {
        /// 帖子 PID
        pid: i64,
        /// 回复内容（不提供则进入交互式输入）
        #[arg(short, long)]
        text: Option<String>,
        /// 引用某条评论的 CID
        #[arg(short, long)]
        quote: Option<i64>,
        /// 图片路径（评论仅限一张）
        #[arg(short, long)]
        image: Option<std::path::PathBuf>,
    },

    /// 点赞帖子
    Like {
        /// 帖子 PID
        pid: i64,
    },
    /// 踩帖子
    Tread {
        /// 帖子 PID
        pid: i64,
    },
    /// 收藏帖子
    Star {
        /// 帖子 PID
        pid: i64,
    },
    /// 取消收藏
    Unstar {
        /// 帖子 PID
        pid: i64,
    },
    /// 查看收藏列表
    Stars {
        #[arg(short, long, default_value = "1")]
        page: u32,
        #[arg(short = 'n', long, default_value = "20")]
        limit: u32,
    },
    /// 关注帖子
    Follow {
        /// 帖子 PID
        pid: i64,
    },
    /// 取消关注
    Unfollow {
        /// 帖子 PID
        pid: i64,
    },

    /// 查看消息通知
    Msg {
        /// 页码
        #[arg(short, long, default_value = "1")]
        page: u32,
        /// 每页条数
        #[arg(short = 'n', long, default_value = "20")]
        limit: u32,
    },
    /// 标记消息为已读
    Read {
        /// 消息 ID 列表
        ids: Vec<i64>,
    },

    /// 查看个人信息
    Me {
        /// 同时显示我的帖子
        #[arg(long)]
        posts: bool,
        /// 页码（我的帖子）
        #[arg(short, long, default_value = "1")]
        page: u32,
        /// 每页条数（我的帖子）
        #[arg(short = 'n', long, default_value = "10")]
        limit: u32,
    },

    /// 举报帖子
    Report {
        /// 帖子 PID
        pid: i64,
        /// 举报原因
        reason: String,
    },

    /// 查询成绩（带颜色渲染）
    Score {
        /// 只显示指定学期，格式如 "25-26-1"
        #[arg(short, long)]
        semester: Option<String>,
        /// 不显示颜色
        #[arg(long)]
        no_color: bool,
    },
    /// 查看课表
    Course {
        /// 同时显示作息时间
        #[arg(long)]
        times: bool,
    },
    /// 查看学术日历
    #[command(alias = "academic")]
    AcademicCal {
        /// 起始日期（默认今天），格式 YYYY-MM-DD
        #[arg(short, long)]
        start: Option<String>,
        /// 结束日期（默认30天后），格式 YYYY-MM-DD
        #[arg(short, long)]
        end: Option<String>,
    },
    /// 查看活动日历
    #[command(alias = "activity")]
    ActivityCal {
        /// 起始日期（默认今天），格式 YYYY-MM-DD
        #[arg(short, long)]
        start: Option<String>,
        /// 结束日期（默认明天），格式 YYYY-MM-DD
        #[arg(short, long)]
        end: Option<String>,
        /// 页码
        #[arg(short, long, default_value = "1")]
        page: u32,
        /// 每页条数
        #[arg(short = 'n', long, default_value = "10")]
        limit: u32,
    },
    /// 查看本周日程
    Schedule {
        /// 起始日期（默认本周一），格式 YYYY-MM-DD
        #[arg(short, long)]
        start: Option<String>,
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
        #[arg(short, long)]
        username: Option<String>,
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
        Commands::Login { password, username, open } => {
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
        Commands::List { feed, page, limit } => commands::cmd_list(&feed, page, limit).await?,
        Commands::Show { pid } => commands::cmd_show(pid).await?,
        Commands::Search { keyword, page, limit } => {
            commands::cmd_search(&keyword, page, limit).await?
        }

        // ── Create ──
        Commands::Post { text, tag, named, fold, reward, image } => {
            commands::cmd_post(text, tag, named, fold, reward, image).await?
        }
        Commands::Reply { pid, text, quote, image } => {
            commands::cmd_reply(pid, text, quote, image).await?
        }

        // ── Interact ──
        Commands::Like { pid } => commands::cmd_like(pid).await?,
        Commands::Tread { pid } => commands::cmd_tread(pid).await?,
        Commands::Star { pid } => commands::cmd_star(pid).await?,
        Commands::Unstar { pid } => commands::cmd_unstar(pid).await?,
        Commands::Stars { page, limit } => commands::cmd_stars(page, limit).await?,
        Commands::Follow { pid } => commands::cmd_follow(pid).await?,
        Commands::Unfollow { pid } => commands::cmd_unfollow(pid).await?,

        // ── Messages ──
        Commands::Msg { page, limit } => commands::cmd_msg(page, limit).await?,
        Commands::Read { ids } => commands::cmd_msg_read(ids).await?,

        // ── User ──
        Commands::Me { posts, page, limit } => commands::cmd_me(posts, page, limit).await?,
        Commands::Report { pid, reason } => commands::cmd_report(pid, &reason).await?,

        // ── 洞天 & 成绩 ──
        Commands::Score { semester, no_color } => {
            commands::cmd_score(semester.as_deref(), no_color).await?
        }
        Commands::Course { times } => commands::cmd_course(times).await?,
        Commands::AcademicCal { start, end } => {
            commands::cmd_academic_cal(start.as_deref(), end.as_deref()).await?
        }
        Commands::ActivityCal { start, end, page, limit } => {
            commands::cmd_activity_cal(start.as_deref(), end.as_deref(), page, limit).await?
        }
        Commands::Schedule { start } => commands::cmd_schedule(start.as_deref()).await?,

        // ── OTP ──
        Commands::Otp { action } => {
            let store = info_common::session::Store::new("treehole")?;
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
