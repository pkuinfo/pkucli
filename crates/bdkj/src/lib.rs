//! 北大空间（学术研讨教室预约）CLI
//!
//! 覆盖完整预约流程：
//! - IAAA 登录（appID=bdkj）
//! - 列出教学楼下的教室 / 教室已被预约时段
//! - 查询学生信息
//! - 提交预约 / 取消预约
//! - 列出个人申请记录

pub mod api;
mod client;
pub mod commands;
mod display;
pub mod groups;
pub mod login;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "bdkj", about = "北大空间（学术研讨教室预约）CLI", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// 登录北大空间（通过 IAAA 统一身份认证）
    Login {
        /// 使用用户名密码登录（默认扫码）
        #[arg(short, long)]
        password: bool,
        /// 学号/职工号
        #[arg(short, long)]
        username: Option<String>,
        /// 用系统图片查看器打开二维码
        #[arg(long)]
        open: bool,
    },
    /// 查看当前登录状态
    Status,
    /// 退出登录
    Logout,

    /// 列出教学楼下的教室（二教 / 四教 / 地学）
    Rooms {
        /// 教学楼名：二教、四教、地学
        building: String,
    },

    /// 查询某教室已被预约的时段
    History {
        /// 教室 ID
        room_id: String,
    },

    /// 查询学生信息（会触发 /classRoom/seachStudent）
    Student {
        /// 学号
        serial: String,
        /// 姓名
        name: String,
    },

    /// 列出当前登录用户的预约记录
    #[command(alias = "ls")]
    List,

    /// 提交一次教室预约
    Reserve {
        /// 教室 ID（可用 `bdkj rooms 二教` 查询）
        #[arg(long)]
        room_id: String,
        /// 起始时间，格式 `YYYY-MM-DD HH:MM:SS`
        #[arg(long)]
        begin: String,
        /// 结束时间，格式 `YYYY-MM-DD HH:MM:SS`
        #[arg(long)]
        end: String,
        /// 预约事由
        #[arg(long)]
        reason: String,
        /// 使用已保存的分组作为参与人（见 `bdkj group set`）
        #[arg(long, short = 'g')]
        group: Option<String>,
        /// 参与人，格式 `学号:姓名`，可重复。申请人自己也必须包含在内。
        /// 与 `--group` 互斥；两者必须至少指定其一。
        #[arg(long = "participant", short = 'p')]
        participants: Vec<String>,
    },

    /// 取消一次预约
    Cancel {
        /// 申请 id（从 `bdkj list` 获取）
        apply_id: String,
    },

    /// 管理参与人分组（便于重复使用固定的一组同学）
    Group {
        #[command(subcommand)]
        action: GroupAction,
    },
}

#[derive(Subcommand)]
pub enum GroupAction {
    /// 列出所有已保存的分组
    List,
    /// 查看某个分组的成员
    Show { name: String },
    /// 保存（或覆盖）一个分组
    Set {
        /// 分组名
        name: String,
        /// 成员，格式 `学号:姓名`，可重复
        #[arg(long = "participant", short = 'p', required = true)]
        participants: Vec<String>,
    },
    /// 删除一个分组
    Remove { name: String },
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
        Commands::Rooms { building } => commands::cmd_rooms(&building).await?,
        Commands::History { room_id } => commands::cmd_history(&room_id).await?,
        Commands::Student { serial, name } => commands::cmd_search_student(&serial, &name).await?,
        Commands::List => commands::cmd_list().await?,
        Commands::Reserve {
            room_id,
            begin,
            end,
            reason,
            group,
            participants,
        } => {
            commands::cmd_reserve(
                &room_id,
                &begin,
                &end,
                &reason,
                group.as_deref(),
                &participants,
            )
            .await?;
        }
        Commands::Cancel { apply_id } => commands::cmd_cancel(&apply_id).await?,
        Commands::Group { action } => commands::cmd_group(action).await?,
    }
    Ok(())
}
