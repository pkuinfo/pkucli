//! 终端格式化输出

use crate::api::{lib_label, DocLib, Entry, Listing};
use colored::Colorize;

pub fn fmt_size(bytes: i64) -> String {
    let b = bytes.max(0) as f64;
    if b < 1024.0 {
        format!("{b:.0} B")
    } else if b < 1024.0 * 1024.0 {
        format!("{:.1} KB", b / 1024.0)
    } else if b < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB", b / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", b / (1024.0 * 1024.0 * 1024.0))
    }
}

/// 网盘的时间戳是微秒
fn fmt_time(micros: Option<i64>) -> String {
    micros
        .and_then(chrono::DateTime::from_timestamp_micros)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

/// 顶层文档库，按类型分组；序号可以直接用作 `disk ls <序号>`
pub fn print_libraries(libs: &[DocLib]) {
    if libs.is_empty() {
        println!("{}", "没有可访问的文档库".dimmed());
        return;
    }
    let mut last_kind = "";
    for (i, l) in libs.iter().enumerate() {
        if l.kind != last_kind {
            println!("{}", lib_label(&l.kind).bold());
            last_kind = &l.kind;
        }
        println!("  {:>2}. {}", i + 1, l.name);
    }
    println!(
        "\n{}",
        "用 `disk ls <文档库>/<文件夹>` 继续浏览；自己的个人文档库可写作「个人」".dimmed()
    );
}

pub fn print_listing(title: &str, listing: &Listing) {
    println!("{}", title.bold());
    if listing.dirs.is_empty() && listing.files.is_empty() {
        println!("  {}", "（空）".dimmed());
        return;
    }
    for d in &listing.dirs {
        println!(
            "  {:>10}  {}  {}",
            "",
            fmt_time(d.modified).dimmed(),
            format!("{}/", d.name).blue().bold()
        );
    }
    for f in &listing.files {
        println!(
            "  {:>10}  {}  {}",
            fmt_size(f.size),
            fmt_time(f.modified).dimmed(),
            f.name
        );
    }
    println!(
        "\n{} 个文件夹，{} 个文件",
        listing.dirs.len(),
        listing.files.len()
    );
}

pub fn print_file(path: &str, e: &Entry) {
    println!("{}", path.bold());
    println!("  大小  {}", fmt_size(e.size));
    println!("  修改  {}", fmt_time(e.modified));
}
