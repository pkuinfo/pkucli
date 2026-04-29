use clap::{Parser, Subcommand};
use std::ffi::OsString;

const PKU_CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

/// (display name, crate name on crates.io, embedded version)
const SUB_VERSIONS: &[(&str, &str, &str)] = &[
    ("treehole", "pku-treehole", pku_treehole::VERSION),
    ("course", "pku-course", pku_course::VERSION),
    ("campuscard", "pku-campuscard", pku_campuscard::VERSION),
    ("elective", "pku-elective", pku_elective::VERSION),
    ("info-auth", "pku-auth", pku_auth::VERSION),
    ("info-spider", "pkuinfo-spider", pkuinfo_spider::VERSION),
    ("claspider", "pku-claspider", pku_claspider::VERSION),
    ("bdkj", "pku-bdkj", pku_bdkj::VERSION),
    ("cwfw", "pku-cwfw", pku_cwfw::VERSION),
    ("portal", "pku-portal", pku_portal::VERSION),
];

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
    /// 列出 pku-cli 自身和所有捆绑子工具的版本
    Version,
    /// 检查并升级 pku-cli（连同所有子 crate）到 crates.io 最新版
    Update {
        /// 仅检查是否有新版，不实际安装
        #[arg(long)]
        check: bool,
        /// 跳过 cargo install --locked 的 lockfile 检查
        #[arg(long)]
        no_locked: bool,
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
        Tools::Treehole { args } => pku_treehole::run_from(prepend_name("treehole", &args)).await,
        Tools::Course { args } => pku_course::run_from(prepend_name("course", &args)).await,
        Tools::Campuscard { args } => {
            pku_campuscard::run_from(prepend_name("campuscard", &args)).await
        }
        Tools::Elective { args } => pku_elective::run_from(prepend_name("elective", &args)).await,
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
        Tools::Version => {
            print_versions();
            Ok(())
        }
        Tools::Update { check, no_locked } => run_update(check, no_locked).await,
    };
    handle_clap_error(result)
}

fn print_versions() {
    println!("pku-cli {PKU_CLI_VERSION}");
    let max_name = SUB_VERSIONS
        .iter()
        .map(|(n, _, _)| n.len())
        .max()
        .unwrap_or(0);
    for (display, crate_name, ver) in SUB_VERSIONS {
        println!("  {display:<max_name$}  {ver:<8}  ({crate_name})");
    }
}

async fn run_update(check_only: bool, no_locked: bool) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::process::Command;

    println!("[*] 查询 crates.io 上 pku-cli 的最新版...");
    // 显式 --registry crates-io，兼容 USTC/RsProxy 等镜像源场景
    let mut output = tokio::task::spawn_blocking(|| {
        Command::new("cargo")
            .args(["search", "pku-cli", "--limit", "1"])
            .output()
    })
    .await?
    .context("执行 `cargo search pku-cli` 失败（请确认 cargo 已安装）")?;
    if !output.status.success()
        && String::from_utf8_lossy(&output.stderr).contains("crates-io is replaced")
    {
        output = tokio::task::spawn_blocking(|| {
            Command::new("cargo")
                .args([
                    "search",
                    "pku-cli",
                    "--limit",
                    "1",
                    "--registry",
                    "crates-io",
                ])
                .output()
        })
        .await?
        .context("执行 `cargo search` (--registry crates-io) 失败")?;
    }
    if !output.status.success() {
        anyhow::bail!(
            "cargo search 失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    // 输出形如：`pku-cli = "0.1.4"    # PKU 命令行工具集...`
    let latest = stdout
        .lines()
        .find(|l| l.starts_with("pku-cli "))
        .and_then(|l| l.split('"').nth(1))
        .context("未能从 cargo search 输出解析出最新版本")?;

    println!("    本地: {PKU_CLI_VERSION}");
    println!("    crates.io: {latest}");

    if latest == PKU_CLI_VERSION {
        println!("[=] 已是最新版本，无需升级。");
        return Ok(());
    }

    if check_only {
        println!("[!] 发现新版本 {latest}，运行 `pku update` 完成升级。");
        return Ok(());
    }

    println!("[*] 正在执行 `cargo install pku-cli --force{}`...", if no_locked { "" } else { " --locked" });
    let mut args = vec!["install", "pku-cli", "--force"];
    if !no_locked {
        args.push("--locked");
    }
    let status = tokio::task::spawn_blocking(move || {
        Command::new("cargo")
            .args(&args)
            .status()
    })
    .await?
    .context("执行 cargo install 失败")?;

    if !status.success() {
        anyhow::bail!("cargo install 退出码非零；可尝试 `pku update --no-locked` 重试");
    }
    println!("[+] 升级完成。");
    Ok(())
}
