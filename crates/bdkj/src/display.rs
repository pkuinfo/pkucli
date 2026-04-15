//! 终端渲染

use crate::api::{Application, HistoryTime, Room};
use colored::Colorize;

pub fn render_rooms(building: &str, rooms: &[Room]) {
    println!(
        "{} {} 下共 {} 间教室",
        "●".green(),
        building.bold(),
        rooms.len()
    );
    println!(
        "  {:<22} {:<10} {:<8} {:<8} {:<8} 锁",
        "ID", "名称", "容量", "热度", "可约"
    );
    for r in rooms {
        let bookable = if r.bookable {
            "是".green()
        } else {
            "否".red()
        };
        println!(
            "  {:<22} {:<10} {:<8} {:<8} {:<8} {}",
            r.id,
            r.name,
            r.seating_capacity,
            format!("{}%", r.popularity),
            bookable,
            r.locked
        );
    }
}

pub fn render_history(room: &str, list: &[HistoryTime]) {
    println!(
        "{} 教室 {} 已被预约时段（{} 条）",
        "●".green(),
        room.bold(),
        list.len()
    );
    for h in list {
        println!("  {} ~ {}  ({})", h.begin_time, h.end_time, h.intervals);
    }
}

pub fn render_applications(apps: &[Application]) {
    println!("{} 共 {} 条预约记录", "●".green(), apps.len());
    for a in apps {
        let status_colored = match a.status.as_str() {
            "申请成功" | "待签到" => a.status.green(),
            "已结束" => a.status.dimmed(),
            "申请已取消" => a.status.red(),
            _ => a.status.yellow(),
        };
        println!(
            "  [{}] {} {}  —  {}",
            status_colored,
            a.room_name.bold().cyan(),
            a.begin_end,
            a.reason
        );
        if !a.id.is_empty() {
            print!("    id={} 申请人={}", a.id, a.applicant);
            if a.can_cancel {
                print!("  {}", "[可取消]".yellow());
            }
            println!();
        }
    }
}
