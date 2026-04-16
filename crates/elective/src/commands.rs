//! 所有子命令的实现

use crate::api::{ElectiveApi, ValidationResult};
use crate::config::{AutoElectCourse, ElectiveConfig};
use crate::display;
use crate::login::APP_NAME;
use anyhow::{anyhow, Result};
use colored::Colorize;
use pkuinfo_common::captcha::{self, CaptchaConfig};
use pkuinfo_common::session::Store;

// ─── show ─────────────────────────────────────────────────────

pub async fn cmd_show() -> Result<()> {
    let api = ElectiveApi::from_session()?;
    let results = api.get_results().await?;
    display::print_results(&results);
    Ok(())
}

// ─── list ─────────────────────────────────────────────────────

pub async fn cmd_list(page: Option<usize>) -> Result<()> {
    let api = ElectiveApi::from_session()?;
    let (total_pages, elected) = api.get_supply_cancel().await?;

    display::print_elected(&elected);

    let page = page.unwrap_or(0);
    if page >= total_pages {
        return Err(anyhow!(
            "页码 {} 超出范围（共 {} 页）",
            page + 1,
            total_pages,
        ));
    }

    let courses = api.get_supplements(page).await?;
    display::print_supplements(&courses, page, total_pages);

    println!(
        "{}",
        format!(
            "提示: 使用 `elective list --page N` 浏览其他页面 (1-{})",
            total_pages,
        )
        .dimmed()
    );
    Ok(())
}

// ─── set ──────────────────────────────────────────────────────

pub async fn cmd_set() -> Result<()> {
    let store = Store::new(APP_NAME)?;
    let mut cfg = ElectiveConfig::load(store.config_dir())?;

    let api = ElectiveApi::from_session()?;

    println!("{} 正在加载所有补退选课程...", "[*]".cyan());
    let all = api.get_all_supplements().await?;

    if all.is_empty() {
        println!("{}", "补退选列表为空".dimmed());
        return Ok(());
    }

    // 显示所有课程供选择
    for (i, c) in all.iter().enumerate() {
        let full_mark = if c.is_full() {
            " [满]".red().to_string()
        } else {
            String::new()
        };
        println!(
            "  {} {} - {} (班号:{}) [页{}]{}",
            format!("[{}]", i + 1).cyan(),
            c.base.name,
            c.base.teacher,
            c.base.class_id,
            c.page_id + 1,
            full_mark,
        );
    }

    println!("请输入要自动选课的课程编号:");
    let idx = read_index()?.saturating_sub(1);
    let course = all.get(idx).ok_or_else(|| anyhow!("无效的编号"))?;

    // 检查是否已在配置中
    let exists = cfg.auto_elect.iter().any(|c| {
        c.name == course.base.name
            && c.teacher == course.base.teacher
            && c.class_id == course.base.class_id
    });

    if exists {
        println!("{} 该课程已在自动选课列表中", "[info]".yellow());
        return Ok(());
    }

    cfg.auto_elect.push(AutoElectCourse {
        page_id: course.page_id,
        name: course.base.name.clone(),
        teacher: course.base.teacher.clone(),
        class_id: course.base.class_id.clone(),
    });

    cfg.save(store.config_dir())?;
    println!(
        "{} 已添加: {} - {} (班号:{})",
        "✓".green(),
        course.base.name,
        course.base.teacher,
        course.base.class_id,
    );
    Ok(())
}

// ─── unset ────────────────────────────────────────────────────

pub fn cmd_unset() -> Result<()> {
    let store = Store::new(APP_NAME)?;
    let mut cfg = ElectiveConfig::load(store.config_dir())?;

    if cfg.auto_elect.is_empty() {
        println!("{}", "自动选课列表为空".dimmed());
        return Ok(());
    }

    display::print_auto_elect_list(&cfg.auto_elect);
    println!("请输入要移除的编号:");

    let idx = read_index()?.saturating_sub(1);
    if idx >= cfg.auto_elect.len() {
        return Err(anyhow!("无效的编号"));
    }

    let removed = cfg.auto_elect.remove(idx);
    cfg.save(store.config_dir())?;

    println!(
        "{} 已移除: {} - {}",
        "✓".green(),
        removed.name,
        removed.teacher,
    );
    Ok(())
}

// ─── config-captcha ───────────────────────────────────────────

pub fn cmd_config_captcha(backend: &str) -> Result<()> {
    let store = Store::new(APP_NAME)?;
    let mut cfg = ElectiveConfig::load(store.config_dir())?;

    cfg.captcha = match backend {
        "manual" => CaptchaConfig::Manual,
        "utool" => CaptchaConfig::Utool,
        "ttshitu" => {
            print!("TTShiTu 用户名: ");
            std::io::stdout().flush()?;
            let mut username = String::new();
            std::io::stdin().read_line(&mut username)?;
            let username = username.trim().to_string();

            print!("TTShiTu 密码: ");
            std::io::stdout().flush()?;
            let password = rpassword::read_password()?;

            CaptchaConfig::TTShiTu { username, password }
        }
        "yunma" => {
            print!("云码 Token (用户中心密钥): ");
            std::io::stdout().flush()?;
            let mut token = String::new();
            std::io::stdin().read_line(&mut token)?;
            let token = token.trim().to_string();

            if token.is_empty() {
                return Err(anyhow!("Token 不能为空"));
            }

            CaptchaConfig::Yunma { token }
        }
        _ => {
            return Err(anyhow!(
                "未知后端: {backend}。可选: manual, utool, ttshitu, yunma"
            ))
        }
    };

    cfg.save(store.config_dir())?;
    println!("{} 验证码后端已设为: {}", "✓".green(), cfg.captcha);
    Ok(())
}

// ─── launch ───────────────────────────────────────────────────

pub async fn cmd_launch(interval_secs: u64) -> Result<()> {
    let store = Store::new(APP_NAME)?;
    let cfg = ElectiveConfig::load(store.config_dir())?;

    if cfg.auto_elect.is_empty() {
        return Err(anyhow!("未配置自动选课目标。使用 `elective set` 添加课程"));
    }

    println!(
        "{} 开始监控 {} 门课程，间隔 {}s",
        "[launch]".green().bold(),
        cfg.auto_elect.len(),
        interval_secs,
    );

    display::print_auto_elect_list(&cfg.auto_elect);
    println!("验证码后端: {}", cfg.captcha);
    println!();

    let api = ElectiveApi::from_session()?;
    let mut targets = cfg.auto_elect.clone();

    loop {
        if targets.is_empty() {
            println!("{} 所有目标课程已选上！", "✓".green().bold());
            break;
        }

        // 检查已选课程，移除已成功的
        match api.get_supply_cancel().await {
            Ok((_, elected)) => {
                targets.retain(|t| {
                    let already = elected.iter().any(|e| {
                        e.name == t.name && e.teacher == t.teacher && e.class_id == t.class_id
                    });
                    if already {
                        println!("  {} 已选上: {} - {}", "✓".green(), t.name, t.teacher,);
                    }
                    !already
                });
            }
            Err(e) => {
                println!("  {} 查询已选课程失败: {e:#}", "[warn]".yellow(),);
            }
        }

        if targets.is_empty() {
            println!("{} 所有目标课程已选上！", "✓".green().bold());
            break;
        }

        // 逐个检查目标课程
        for target in &targets {
            match api.get_supplements(target.page_id).await {
                Ok(supplements) => {
                    let found = supplements.iter().find(|s| {
                        s.base.name == target.name
                            && s.base.teacher == target.teacher
                            && s.base.class_id == target.class_id
                    });

                    if let Some(course) = found {
                        if course.is_full() {
                            println!(
                                "  {} {} - {} ({})",
                                "×".red(),
                                target.name,
                                target.teacher,
                                course.base.status.dimmed(),
                            );
                            continue;
                        }

                        // 有名额！尝试选课
                        println!(
                            "  {} {} - {} 有名额！正在尝试选课...",
                            "!".yellow().bold(),
                            target.name,
                            target.teacher,
                        );

                        match try_elect(&api, &cfg.captcha, &store, course.elect_url.clone()).await
                        {
                            Ok(true) => {
                                println!(
                                    "  {} 选课成功: {} - {}",
                                    "✓".green().bold(),
                                    target.name,
                                    target.teacher,
                                );
                            }
                            Ok(false) => {
                                println!("  {} 选课失败（服务器拒绝）", "[fail]".red(),);
                            }
                            Err(e) => {
                                println!("  {} 选课出错: {e:#}", "[error]".red(),);
                            }
                        }
                    } else {
                        println!(
                            "  {} {} - {} 在第 {} 页未找到",
                            "?".yellow(),
                            target.name,
                            target.teacher,
                            target.page_id + 1,
                        );
                    }
                }
                Err(e) => {
                    println!(
                        "  {} 获取第 {} 页失败: {e:#}",
                        "[warn]".yellow(),
                        target.page_id + 1,
                    );
                }
            }
        }

        let now = chrono::Local::now().format("%H:%M:%S");
        println!(
            "  {} 下次检查于 {}s 后 ({})",
            "[wait]".dimmed(),
            interval_secs,
            now,
        );
        tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
    }

    Ok(())
}

/// 尝试通过验证码并选课
async fn try_elect(
    api: &ElectiveApi,
    captcha_cfg: &CaptchaConfig,
    store: &Store,
    elect_url: String,
) -> Result<bool> {
    // 最多重试 3 次验证码
    for attempt in 0..3 {
        // 获取验证码图片
        let image = api.get_captcha_image().await?;

        // 识别验证码
        let code =
            captcha::recognize(api.client(), captcha_cfg, &image, store.config_dir()).await?;

        println!("    验证码识别结果: {code}");

        // 验证
        match api.validate_captcha(&code).await? {
            ValidationResult::Success => {
                // 验证通过，提交选课
                let msg = api.elect(&elect_url).await?;
                if let Some(m) = &msg {
                    println!("    选课结果: {m}");
                }
                return Ok(true);
            }
            ValidationResult::Wrong => {
                println!("    {} 验证码错误 ({}/3)", "[retry]".yellow(), attempt + 1,);
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            ValidationResult::Empty => {
                println!("    {} 验证码未填写", "[error]".red());
            }
        }
    }

    Ok(false)
}

// ─── helpers ──────────────────────────────────────────────────

use std::io::Write;

fn read_index() -> Result<usize> {
    print!("> ");
    std::io::stdout().flush()?;
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf)?;
    buf.trim()
        .parse::<usize>()
        .map_err(|_| anyhow!("请输入有效数字"))
}
