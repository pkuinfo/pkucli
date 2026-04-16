//! 北大课程信息爬取工具
//!
//! 从多个渠道获取当前学期的全部课程信息：
//!
//! 1. **教务部课表查询** (`dean.pku.edu.cn`) — 无需登录，课程最全，但缺教室地点
//! 2. **选课系统课程查询** (`elective.pku.edu.cn`) — 需 IAAA 登录，含教室信息
//! 3. **智云课堂** (`onlineroomse.pku.edu.cn`) — 需 JWT，含精确教室 + 历史录播信息
//!
//! 三个渠道通过 (课程号, 班号) 匹配合并。

pub mod dean;
pub mod display;
pub mod elective_query;
pub mod model;
pub mod zhiyun;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "claspider", about = "北大课程信息爬取工具", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// 从教务部课表查询抓取课程（无需登录）
    Dean {
        /// 学年学期，如 "25-26-2" 表示 25-26学年第2学期
        #[arg(short = 's', long, default_value = "25-26-2")]
        semester: String,
        /// 按院系代码筛选（如 "00048"=信息科学技术学院），不指定则抓全部
        #[arg(short, long)]
        department: Option<String>,
        /// 按课程名关键词搜索
        #[arg(short = 'n', long)]
        name: Option<String>,
        /// 按教师姓名搜索
        #[arg(short, long)]
        teacher: Option<String>,
        /// 输出 JSON 格式
        #[arg(long)]
        json: bool,
    },

    /// 从选课系统抓取课程（需要先 `elective login`）
    Elective {
        /// 课程分类: speciality, politics, english, gym, tsk_choice, pub_choice,
        /// liberal_computer, ldjyk, szxzxbx, education_plan_bk
        #[arg(short, long, default_value = "speciality")]
        category: String,
        /// 按院系代码筛选，不指定则抓全部
        #[arg(short, long)]
        department: Option<String>,
        /// 按课程名关键词搜索
        #[arg(short = 'n', long)]
        name: Option<String>,
        /// 输出 JSON 格式
        #[arg(long)]
        json: bool,
    },

    /// 从智云课堂抓取课程（需要 JWT token）
    Zhiyun {
        /// JWT token（从浏览器 onlineroomse.pku.edu.cn 的 _token cookie 中提取）
        #[arg(short = 't', long, env = "ZHIYUN_JWT")]
        token: String,
        /// 查询周的起始日期（周一），如 "2026-04-13"
        #[arg(short, long)]
        week: String,
        /// 查询课程详情以获取课程代码（较慢，但可用于合并）
        #[arg(long)]
        details: bool,
        /// 输出 JSON 格式
        #[arg(long)]
        json: bool,
    },

    /// 合并多个渠道的数据（教务部 + 选课系统 + 可选智云）
    Merge {
        /// 学年学期
        #[arg(short = 's', long, default_value = "25-26-2")]
        semester: String,
        /// 选课系统课程分类
        #[arg(short, long, default_value = "speciality")]
        category: String,
        /// 按院系代码筛选
        #[arg(short, long)]
        department: Option<String>,
        /// 智云 JWT token（可选，提供则加入三方合并）
        #[arg(long, env = "ZHIYUN_JWT")]
        zhiyun_token: Option<String>,
        /// 智云查询周起始日期
        #[arg(long, default_value = "2026-04-13")]
        zhiyun_week: Option<String>,
        /// 输出 JSON 格式
        #[arg(long)]
        json: bool,
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

async fn dispatch(command: Commands) -> Result<()> {
    match command {
        Commands::Dean {
            semester,
            department,
            name,
            teacher,
            json,
        } => {
            let courses = dean::fetch_all(
                &semester,
                department.as_deref(),
                name.as_deref(),
                teacher.as_deref(),
            )
            .await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&courses)?);
            } else {
                display::print_courses(&courses);
            }
        }

        Commands::Elective {
            category,
            department,
            name,
            json,
        } => {
            let courses =
                elective_query::fetch_all(&category, department.as_deref(), name.as_deref())
                    .await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&courses)?);
            } else {
                display::print_courses(&courses);
            }
        }

        Commands::Zhiyun {
            token,
            week,
            details,
            json,
        } => {
            let courses = zhiyun::fetch_all(&token, &week, details).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&courses)?);
            } else {
                display::print_courses(&courses);
            }
        }

        Commands::Merge {
            semester,
            category,
            department,
            zhiyun_token,
            zhiyun_week,
            json,
        } => {
            let merged = model::merge_sources(
                &semester,
                &category,
                department.as_deref(),
                zhiyun_token.as_deref(),
                zhiyun_week.as_deref(),
            )
            .await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&merged)?);
            } else {
                display::print_courses(&merged);
            }
        }
    }
    Ok(())
}
