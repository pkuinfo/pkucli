//! 命令处理器

use crate::{
    api::{self, BdkjApi, Participant},
    client, display, groups,
    login::{self, APP_NAME},
    GroupAction,
};
use anyhow::{anyhow, Result};
use colored::Colorize;
use pkuinfo_common::session::Store;

fn http() -> Result<reqwest::Client> {
    let _session = login::load_session()?; // 会话过期提示
    let store = Store::new(APP_NAME)?;
    let cookie_store = store.load_cookie_store()?;
    let client = client::build(cookie_store)?;
    Ok(client)
}

pub async fn cmd_rooms(building: &str) -> Result<()> {
    let bid = api::building_id(building)
        .ok_or_else(|| anyhow!("未知教学楼：{building}。支持：二教 / 四教 / 地学"))?;
    let api = BdkjApi::new(http()?);
    let rooms = api.list_rooms(bid).await?;
    display::render_rooms(building, &rooms);
    Ok(())
}

pub async fn cmd_history(room_id: &str) -> Result<()> {
    let api = BdkjApi::new(http()?);
    let start = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let list = api.history_time(room_id, &start).await?;
    display::render_history(room_id, &list);
    Ok(())
}

pub async fn cmd_list() -> Result<()> {
    let api = BdkjApi::new(http()?);
    let apps = api.list_applications().await?;
    display::render_applications(&apps);
    Ok(())
}

pub async fn cmd_search_student(serial: &str, name: &str) -> Result<()> {
    let api = BdkjApi::new(http()?);
    let info = api.search_student(serial, name).await?;
    println!("{} {} ({})", "●".green(), info.name.bold(), info.serial);
    println!("  id       = {}", info.id);
    if let Some(c) = &info.college {
        println!("  学院     = {c}");
    }
    if let Some(p) = &info.mobile_phone {
        println!("  电话     = {p}");
    }
    if let Some(e) = &info.email {
        println!("  邮箱     = {e}");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn cmd_reserve(
    room_id: &str,
    begin_time: &str,
    end_time: &str,
    reason: &str,
    group: Option<&str>,
    participants: &[String],
) -> Result<()> {
    // 参与人来源：--group 或 --participant（两者至少给一个）
    let members = match (group, participants.is_empty()) {
        (Some(_), false) => {
            return Err(anyhow!("--group 与 --participant 不能同时使用"));
        }
        (Some(name), true) => groups::get(name)?,
        (None, false) => groups::parse_members(participants)?,
        (None, true) => {
            return Err(anyhow!(
                "请通过 --group <name> 或 --participant 学号:姓名 指定参与人"
            ))
        }
    };

    let api = BdkjApi::new(http()?);

    let mut ps: Vec<Participant> = Vec::new();
    for m in &members {
        let info = api.search_student(&m.serial, &m.name).await?;
        ps.push(Participant {
            student_id: info.id,
            serial: info.serial,
            name: info.name,
        });
    }
    if ps.len() < 3 {
        return Err(anyhow!(
            "申请教室要求不少于 3 人（包含申请人），当前 {} 人",
            ps.len()
        ));
    }

    println!(
        "{} 提交预约 room={}, {} ~ {}，参与人 {} 人",
        "[*]".cyan(),
        room_id,
        begin_time,
        end_time,
        ps.len()
    );
    let result = api
        .submit_apply(room_id, begin_time, end_time, reason, &ps)
        .await?;
    if result.success {
        println!("{} {}", "[done]".green().bold(), result.message);
    } else {
        return Err(anyhow!("预约失败：{}", result.message));
    }
    Ok(())
}

pub async fn cmd_cancel(apply_id: &str) -> Result<()> {
    let api = BdkjApi::new(http()?);
    api.cancel_apply(apply_id).await?;
    println!("{} 已取消预约 {apply_id}", "[done]".green().bold());
    Ok(())
}

pub async fn cmd_group(action: GroupAction) -> Result<()> {
    match action {
        GroupAction::List => {
            let file = groups::load()?;
            if file.groups.is_empty() {
                println!(
                    "{} 没有任何分组。使用 `bdkj group set <name> -p 学号:姓名 ...`",
                    "○".yellow()
                );
                return Ok(());
            }
            println!("{} 共 {} 个分组", "●".green(), file.groups.len());
            for (name, members) in &file.groups {
                println!(
                    "  {} ({} 人) — {}",
                    name.bold().cyan(),
                    members.len(),
                    members
                        .iter()
                        .map(|m| format!("{}:{}", m.serial, m.name))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
        GroupAction::Show { name } => {
            let members = groups::get(&name)?;
            println!("{} 分组 {}", "●".green(), name.bold().cyan());
            for m in &members {
                println!("  {}  {}", m.serial, m.name);
            }
        }
        GroupAction::Set { name, participants } => {
            let members = groups::parse_members(&participants)?;
            groups::upsert(&name, members)?;
            println!("{} 已保存分组 {}", "✓".green(), name.bold());
        }
        GroupAction::Remove { name } => {
            if groups::remove(&name)? {
                println!("{} 已删除分组 {}", "✓".green(), name.bold());
            } else {
                println!("{} 分组 {} 不存在", "○".yellow(), name);
            }
        }
    }
    Ok(())
}
