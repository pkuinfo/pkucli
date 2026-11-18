//! 北大学生邮箱（@stu.pku.edu.cn，网易企业邮）CLI
//!
//! - 直接用学号 + IAAA 密码登录邮箱（RSA 加密，不经过 IAAA），短信二次验证每台机器一次
//! - 文件夹、邮件列表、阅读、搜索、附件下载、通讯录
//! - 发信 / 回复：默认只预览，加 `--yes` 才真正发送
//!
//! 协议见 `docs/stu-mail-api.md`。

mod api;
mod client;
pub mod commands;
mod display;
pub mod login;
mod parse;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "mail",
    about = "北大学生邮箱（@stu.pku.edu.cn）CLI 客户端",
    version
)]
pub struct Cli {
    /// 以 JSON 输出（便于 AI Agent 解析）
    #[arg(long, global = true)]
    pub json: bool,
    #[command(subcommand)]
    pub command: Commands,
}

/// 正文：二选一
#[derive(Args)]
#[group(required = true, multiple = false)]
pub struct BodyArgs {
    /// 正文文本
    #[arg(long)]
    pub body: Option<String>,
    /// 从文件读正文（`-` 表示标准输入）
    #[arg(long)]
    pub body_file: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// 登录学生邮箱（学号 + IAAA 密码；新设备需短信验证一次）
    Login {
        /// 使用 keyring / 环境变量里的密码登录（邮箱只有密码登录，可省略）
        #[arg(short, long)]
        password: bool,
        /// 学号（keyring / 环境变量都没有凭据时作为交互输入的默认值）
        #[arg(short, long)]
        username: Option<String>,
        /// 短信验证码：完成上一次 `mail login -p` 发出的短信验证
        #[arg(long)]
        code: Option<String>,
    },
    /// 查看当前登录状态
    Status,
    /// 退出登录（保留设备信任）
    Logout,

    /// 列出文件夹及未读数
    Folders,
    /// 列出邮件
    #[command(alias = "list")]
    Ls {
        /// 文件夹：inbox / sent / drafts / trash / spam、中文名或 id
        #[arg(short, long, default_value = "inbox")]
        folder: String,
        /// 只看未读
        #[arg(long)]
        unread: bool,
        /// 页码（从 1 开始）
        #[arg(long, default_value_t = 1)]
        page: usize,
        /// 每页数量（最多 100）
        #[arg(short = 'n', long, default_value_t = 20)]
        limit: usize,
    },
    /// 阅读一封邮件（默认不标记为已读）
    Read {
        /// 邮件 id（`mail ls` 里的 mid）
        mid: String,
        /// 输出原始 HTML 正文
        #[arg(long)]
        html: bool,
        /// 同时标记为已读
        #[arg(long)]
        mark_read: bool,
    },
    /// 下载附件（不指定 part 时下载全部附件）
    #[command(alias = "dl")]
    Download {
        /// 邮件 id
        mid: String,
        /// 附件 part id（`mail read` 里方括号中的编号）
        part: Option<String>,
        /// 输出目录（默认当前目录）
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// 全文搜索（发件人、收件人、主题、正文、附件名）
    Search {
        /// 搜索词
        #[arg(required = true, num_args = 1..)]
        words: Vec<String>,
        /// 限定文件夹
        #[arg(short, long)]
        folder: Option<String>,
        /// 页码（每页 20 封）
        #[arg(long, default_value_t = 1)]
        page: usize,
    },
    /// 个人通讯录（可按姓名 / 邮箱 / 拼音过滤）
    Contacts {
        /// 过滤词
        query: Option<String>,
    },
    /// 写信：默认只预览，加 --yes 才发送
    Send {
        /// 收件人（可重复，或用逗号分隔）
        #[arg(long, required = true)]
        to: Vec<String>,
        /// 抄送
        #[arg(long)]
        cc: Vec<String>,
        /// 密送
        #[arg(long)]
        bcc: Vec<String>,
        /// 主题
        #[arg(short, long)]
        subject: String,
        #[command(flatten)]
        body: BodyArgs,
        /// 正文已经是 HTML（默认按纯文本处理）
        #[arg(long)]
        html: bool,
        /// 确认发送（不加则只预览）
        #[arg(long)]
        yes: bool,
    },
    /// 回复邮件：默认只预览，加 --yes 才发送
    Reply {
        /// 要回复的邮件 id
        mid: String,
        /// 回复全部
        #[arg(long)]
        all: bool,
        #[command(flatten)]
        body: BodyArgs,
        /// 正文已经是 HTML（默认按纯文本处理）
        #[arg(long)]
        html: bool,
        /// 确认发送（不加则只预览）
        #[arg(long)]
        yes: bool,
    },
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    dispatch(cli.command, cli.json).await
}

pub async fn run_from<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::try_parse_from(args)?;
    dispatch(cli.command, cli.json).await
}

pub async fn dispatch(command: Commands, json: bool) -> Result<()> {
    match command {
        Commands::Login {
            password,
            username,
            code,
        } => {
            // `-p` 只为和其他工具保持一致：邮箱没有扫码登录，总是用密码
            let _ = password;
            login::cmd_login(username.as_deref(), code.as_deref(), json).await
        }
        Commands::Status => login::status(json),
        Commands::Logout => login::logout(json),
        Commands::Folders => commands::cmd_folders(json).await,
        Commands::Ls {
            folder,
            unread,
            page,
            limit,
        } => commands::cmd_ls(&folder, unread, page, limit, json).await,
        Commands::Read {
            mid,
            html,
            mark_read,
        } => commands::cmd_read(&mid, html, mark_read, json).await,
        Commands::Download { mid, part, output } => {
            commands::cmd_download(&mid, part.as_deref(), output.as_deref(), json).await
        }
        Commands::Search {
            words,
            folder,
            page,
        } => commands::cmd_search(&words, folder.as_deref(), page, json).await,
        Commands::Contacts { query } => commands::cmd_contacts(query.as_deref(), json).await,
        Commands::Send {
            to,
            cc,
            bcc,
            subject,
            body,
            html,
            yes,
        } => {
            let body = commands::read_body(body.body.as_deref(), body.body_file.as_deref())?;
            commands::cmd_send(
                commands::SendArgs {
                    to: &to,
                    cc: &cc,
                    bcc: &bcc,
                    subject: &subject,
                    body,
                    html,
                    yes,
                },
                json,
            )
            .await
        }
        Commands::Reply {
            mid,
            all,
            body,
            html,
            yes,
        } => {
            let body = commands::read_body(body.body.as_deref(), body.body_file.as_deref())?;
            commands::cmd_reply(&mid, all, body, html, yes, json).await
        }
    }
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
