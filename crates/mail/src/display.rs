//! 终端格式化输出

use crate::api::{Contact, Folder, MessageDetail, MsgSummary};
use crate::parse::display_addr;
use colored::Colorize;

pub fn fmt_size(bytes: i64) -> String {
    let b = bytes.max(0) as f64;
    if b < 1024.0 {
        format!("{b:.0} B")
    } else if b < 1024.0 * 1024.0 {
        format!("{:.1} KB", b / 1024.0)
    } else {
        format!("{:.1} MB", b / (1024.0 * 1024.0))
    }
}

/// `2026-11-17 09:00:02` → `2026-11-17 09:00`
fn short_date(s: &str) -> &str {
    s.get(..16).unwrap_or(s)
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}

/// 发件人只显示名字（没有名字时显示地址）
fn sender_name(from: &str) -> String {
    let from = display_addr(from);
    match from.find('<') {
        Some(i) if i > 0 => from[..i].trim().to_string(),
        _ => from.trim_matches(['<', '>']).to_string(),
    }
}

pub fn print_folders(folders: &[Folder]) {
    println!("{:>6}  {:<16} {:>8} {:>8}", "id", "文件夹", "邮件", "未读");
    for f in folders {
        let unread = if f.unread > 0 {
            f.unread.to_string().yellow().bold().to_string()
        } else {
            "0".dimmed().to_string()
        };
        println!(
            "{:>6}  {:<16} {:>8} {:>8}",
            f.id,
            truncate(&f.name, 16),
            f.total,
            unread
        );
    }
}

pub fn print_messages(title: &str, list: &[MsgSummary], total: i64, page: usize, limit: usize) {
    println!("{}", title.bold());
    if list.is_empty() {
        println!("  {}", "（没有邮件）".dimmed());
        return;
    }
    for m in list {
        let dot = if m.read { " ".normal() } else { "●".yellow() };
        let clip = if m.attached { "📎" } else { "  " };
        let subject = if m.subject.is_empty() {
            "（无主题）".to_string()
        } else {
            truncate(&m.subject, 40)
        };
        let subject = if m.read {
            subject.normal()
        } else {
            subject.bold()
        };
        println!(
            "{dot} {}  {:<14} {clip} {subject}",
            short_date(&m.date).dimmed(),
            truncate(&sender_name(&m.from), 14),
        );
        println!("    {}", format!("mid: {}", m.id).dimmed());
    }
    let pages = (total.max(0) as usize).div_ceil(limit.max(1));
    println!(
        "\n第 {page}/{} 页，共 {total} 封。用 `mail read <mid>` 阅读",
        pages.max(1)
    );
}

pub fn print_message(m: &MessageDetail, raw_html: bool) {
    println!("{}", m.subject.bold());
    println!("{} {}", "发件人:".dimmed(), display_addr(&m.from));
    if !m.to.is_empty() {
        let to: Vec<String> = m.to.iter().map(|a| display_addr(a)).collect();
        println!("{} {}", "收件人:".dimmed(), to.join(", "));
    }
    if !m.cc.is_empty() {
        let cc: Vec<String> = m.cc.iter().map(|a| display_addr(a)).collect();
        println!("{} {}", "抄送:  ".dimmed(), cc.join(", "));
    }
    println!("{} {}", "时间:  ".dimmed(), m.date);
    println!("{}", "─".repeat(60).dimmed());
    match (&m.html, raw_html) {
        (Some(html), true) => println!("{html}"),
        _ if m.text.is_empty() => println!("{}", "（正文为空）".dimmed()),
        _ => println!("{}", m.text),
    }
    if !m.attachments.is_empty() {
        println!("{}", "─".repeat(60).dimmed());
        println!("{}", "附件:".bold());
        for a in &m.attachments {
            println!(
                "  [{}] {}  {}",
                a.id.cyan(),
                a.filename,
                fmt_size(a.size).dimmed()
            );
        }
        println!(
            "{}",
            format!("用 `mail download {} [part]` 下载", m.id).dimmed()
        );
    }
}

pub fn print_contacts(list: &[Contact]) {
    if list.is_empty() {
        println!("{}", "没有匹配的联系人".dimmed());
        return;
    }
    for c in list {
        let mut extra = Vec::new();
        if !c.org.is_empty() {
            extra.push(c.org.clone());
        }
        if !c.remark.is_empty() {
            extra.push(c.remark.clone());
        }
        if !c.mobile.is_empty() {
            extra.push(c.mobile.join("/"));
        }
        println!(
            "  {:<10} {}  {}",
            c.name.bold(),
            c.email.cyan(),
            extra.join(" · ").dimmed()
        );
    }
    println!("\n共 {} 位联系人", list.len());
}

/// 发信 / 回复预览
pub struct Preview<'a> {
    pub kind: &'a str,
    pub from: &'a str,
    pub to: &'a [String],
    pub cc: &'a [String],
    pub bcc: &'a [String],
    pub subject: &'a str,
    pub body: &'a str,
    pub note: Option<&'a str>,
}

pub fn print_preview(p: &Preview) {
    println!(
        "{}",
        format!("── {}预览（尚未发送）──", p.kind).yellow().bold()
    );
    println!("{} {}", "发件人:".dimmed(), p.from);
    let to: Vec<String> = p.to.iter().map(|a| display_addr(a)).collect();
    println!("{} {}", "收件人:".dimmed(), to.join(", "));
    if !p.cc.is_empty() {
        let cc: Vec<String> = p.cc.iter().map(|a| display_addr(a)).collect();
        println!("{} {}", "抄送:  ".dimmed(), cc.join(", "));
    }
    if !p.bcc.is_empty() {
        println!("{} {}", "密送:  ".dimmed(), p.bcc.join(", "));
    }
    println!("{} {}", "主题:  ".dimmed(), p.subject);
    println!("{}", "─".repeat(60).dimmed());
    println!("{}", p.body);
    if let Some(note) = p.note {
        println!("{}", note.dimmed());
    }
    println!("{}", "─".repeat(60).dimmed());
}
