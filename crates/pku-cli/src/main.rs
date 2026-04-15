use clap::{Parser, Subcommand};
use std::ffi::OsString;

#[derive(Parser)]
#[command(
    name = "pku",
    about = "PKU 命令行工具集",
    version,
    subcommand_required = true,
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Tools,
}

#[derive(Subcommand)]
enum Tools {
    /// 北大树洞 — 匿名论坛
    #[command(alias = "th", disable_help_flag = true, disable_version_flag = true)]
    Treehole {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// 北大教学网 — Blackboard Learn
    #[command(disable_help_flag = true, disable_version_flag = true)]
    Course {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// 北大校园卡 — 余额、充值、账单
    #[command(alias = "card", disable_help_flag = true, disable_version_flag = true)]
    Campuscard {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// 北大选课网 — 自动选课
    #[command(disable_help_flag = true, disable_version_flag = true)]
    Elective {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// 凭据管理 — 安全存储 IAAA 密码
    #[command(disable_help_flag = true, disable_version_flag = true)]
    Auth {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// 微信公众号文章爬虫
    #[command(disable_help_flag = true, disable_version_flag = true)]
    Spider {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// 课程信息爬取（教务部 + 选课网）
    #[command(alias = "cs", disable_help_flag = true, disable_version_flag = true)]
    Claspider {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// 北大空间 — 学术研讨教室预约
    #[command(disable_help_flag = true, disable_version_flag = true)]
    Bdkj {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// 财务综合信息门户 — 个人酬金等
    #[command(disable_help_flag = true, disable_version_flag = true)]
    Cwfw {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// 校内信息门户 — 空闲教室 / 校历 / 网费
    #[command(disable_help_flag = true, disable_version_flag = true)]
    Portal {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
}

fn prepend_name(name: &str, args: &[OsString]) -> Vec<OsString> {
    std::iter::once(OsString::from(name))
        .chain(args.iter().cloned())
        .collect()
}

/// Unwrap clap's DisplayHelp/DisplayVersion errors so they print cleanly.
/// Sub-crates use `try_parse_from(...)?` which propagates clap errors through
/// `anyhow::Error`. Without unwrapping, `--help` and `--version` would appear
/// prefixed with "Error:" in the terminal.
fn handle_clap_error(result: anyhow::Result<()>) -> anyhow::Result<()> {
    if let Err(e) = &result {
        if let Some(clap_err) = e.downcast_ref::<clap::Error>() {
            let _ = clap_err.print();
            match clap_err.kind() {
                clap::error::ErrorKind::DisplayHelp
                | clap::error::ErrorKind::DisplayVersion
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                    std::process::exit(0);
                }
                _ => std::process::exit(1),
            }
        }
    }
    result
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let result = match cli.command {
        Tools::Treehole { args } => {
            pku_treehole::run_from(prepend_name("treehole", &args)).await
        }
        Tools::Course { args } => pku_course::run_from(prepend_name("course", &args)).await,
        Tools::Campuscard { args } => {
            pku_campuscard::run_from(prepend_name("campuscard", &args)).await
        }
        Tools::Elective { args } => {
            pku_elective::run_from(prepend_name("elective", &args)).await
        }
        Tools::Auth { args } => pku_auth::run_from(prepend_name("info-auth", &args)),
        Tools::Spider { args } => {
            pkuinfo_spider::run_from(prepend_name("info-spider", &args)).await
        }
        Tools::Claspider { args } => {
            pku_claspider::run_from(prepend_name("claspider", &args)).await
        }
        Tools::Bdkj { args } => pku_bdkj::run_from(prepend_name("bdkj", &args)).await,
        Tools::Cwfw { args } => pku_cwfw::run_from(prepend_name("cwfw", &args)).await,
        Tools::Portal { args } => pku_portal::run_from(prepend_name("portal", &args)).await,
    };
    handle_clap_error(result)
}
