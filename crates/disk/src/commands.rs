//! 子命令实现

use crate::api::{lib_label, DiskApi, DocLib, Entry, LIB_TYPES};
use crate::display;
use anyhow::{anyhow, bail, Result};
use colored::Colorize;
use std::path::{Path, PathBuf};

/// 指自己个人文档库的简写（它的真实名字是「姓名_学号」）
const PERSONAL_ALIASES: [&str; 4] = ["个人", "我的", "个人文档", "个人文档库"];

/// 路径解析的结果
enum Node {
    Lib(DocLib),
    Dir(Entry),
    File(Entry),
}

/// 第一段路径 → 文档库：名字 > 「个人」等简写 > 类型名（该类型只有一个时）> 序号 > 唯一的部分匹配
fn pick_lib<'a>(libs: &'a [DocLib], seg: &str) -> Result<&'a DocLib> {
    if let Some(l) = libs.iter().find(|l| l.name == seg) {
        return Ok(l);
    }
    let kind = if PERSONAL_ALIASES.contains(&seg) {
        Some("user_doc_lib")
    } else {
        LIB_TYPES.iter().find(|(_, v)| *v == seg).map(|(k, _)| *k)
    };
    if let Some(kind) = kind {
        let hits: Vec<_> = libs.iter().filter(|l| l.kind == kind).collect();
        return match hits.as_slice() {
            [one] => Ok(one),
            [] => bail!("你没有「{}」", lib_label(kind)),
            _ => bail!(
                "有 {} 个「{}」：{}，请写具体名字",
                hits.len(),
                lib_label(kind),
                hits.iter()
                    .map(|l| l.name.as_str())
                    .collect::<Vec<_>>()
                    .join("、")
            ),
        };
    }
    if let Ok(i) = seg.parse::<usize>() {
        return libs
            .get(i.wrapping_sub(1))
            .ok_or_else(|| anyhow!("没有第 {i} 个文档库，共 {} 个", libs.len()));
    }
    let hits: Vec<_> = libs.iter().filter(|l| l.name.contains(seg)).collect();
    match hits.as_slice() {
        [one] => Ok(one),
        [] => bail!("没有名为「{seg}」的文档库。用 `disk ls` 查看"),
        _ => bail!("有 {} 个文档库名字包含「{seg}」，请写完整", hits.len()),
    }
}

async fn resolve(api: &DiskApi, path: &str) -> Result<(Node, String)> {
    let segs: Vec<&str> = path.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    let Some((first, rest)) = segs.split_first() else {
        bail!("路径为空");
    };
    let libs = api.libraries().await?;
    let lib = pick_lib(&libs, first)?.clone();
    let mut shown = vec![lib.name.clone()];
    let mut node = Node::Lib(lib);
    for seg in rest {
        let docid = match &node {
            Node::Lib(l) => l.id.clone(),
            Node::Dir(e) => e.docid.clone(),
            Node::File(e) => bail!("「{}」是文件，下面没有「{seg}」", e.name),
        };
        let listing = api.list(&docid).await?;
        node = if let Some(d) = listing.dirs.into_iter().find(|d| d.name == *seg) {
            Node::Dir(d)
        } else if let Some(f) = listing.files.into_iter().find(|f| f.name == *seg) {
            Node::File(f)
        } else {
            bail!("「{}」下没有「{seg}」", shown.join("/"));
        };
        shown.push((*seg).to_string());
    }
    Ok((node, shown.join("/")))
}

// ─── ls ────────────────────────────────────────────────────────

pub async fn cmd_ls(path: Option<&str>) -> Result<()> {
    let api = DiskApi::from_session()?;
    let Some(path) = path else {
        display::print_libraries(&api.libraries().await?);
        return Ok(());
    };
    let (node, shown) = resolve(&api, path).await?;
    match node {
        Node::File(e) => display::print_file(&shown, &e),
        Node::Lib(l) => display::print_listing(&shown, &api.list(&l.id).await?),
        Node::Dir(d) => display::print_listing(&shown, &api.list(&d.docid).await?),
    }
    Ok(())
}

// ─── download ──────────────────────────────────────────────────

/// Windows 不允许的文件名字符换成下划线
fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if r#"\/:*?"<>|"#.contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    s.trim_end_matches(['.', ' ']).to_string()
}

#[derive(Default)]
struct Stats {
    downloaded: usize,
    skipped: usize,
    bytes: u64,
}

async fn fetch_one(api: &DiskApi, e: &Entry, dest: &Path, stats: &mut Stats) -> Result<()> {
    if let Ok(meta) = std::fs::metadata(dest) {
        if meta.is_file() && meta.len() as i64 == e.size {
            stats.skipped += 1;
            return Ok(());
        }
    }
    let n = api.download_to(&e.docid, dest).await?;
    println!(
        "  {} {} {}",
        "↓".green(),
        dest.display(),
        display::fmt_size(n as i64).dimmed()
    );
    stats.downloaded += 1;
    stats.bytes += n;
    Ok(())
}

/// 下载一个文件夹（或整个文档库）：逐层列目录，保持目录结构
async fn fetch_tree(api: &DiskApi, docid: &str, root: PathBuf, stats: &mut Stats) -> Result<()> {
    let mut stack = vec![(docid.to_string(), root)];
    while let Some((id, dir)) = stack.pop() {
        std::fs::create_dir_all(&dir)?;
        let listing = api.list(&id).await?;
        for f in &listing.files {
            fetch_one(api, f, &dir.join(sanitize(&f.name)), stats).await?;
        }
        for d in listing.dirs.into_iter().rev() {
            let sub = dir.join(sanitize(&d.name));
            stack.push((d.docid, sub));
        }
    }
    Ok(())
}

pub async fn cmd_download(path: &str, output: Option<&Path>) -> Result<()> {
    let api = DiskApi::from_session()?;
    let (node, shown) = resolve(&api, path).await?;
    let out = output.map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    println!("{} {}", "下载".bold(), shown);

    let mut stats = Stats::default();
    match node {
        Node::File(e) => {
            let dest = out.join(sanitize(&e.name));
            fetch_one(&api, &e, &dest, &mut stats).await?;
        }
        Node::Lib(l) => fetch_tree(&api, &l.id, out.join(sanitize(&l.name)), &mut stats).await?,
        Node::Dir(d) => fetch_tree(&api, &d.docid, out.join(sanitize(&d.name)), &mut stats).await?,
    }
    println!(
        "{} 下载 {} 个文件（{}），跳过已存在的 {} 个 → {}",
        "✓".green(),
        stats.downloaded,
        display::fmt_size(stats.bytes as i64),
        stats.skipped,
        out.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib(name: &str, kind: &str) -> DocLib {
        DocLib {
            id: format!("gns://{name}"),
            name: name.into(),
            kind: kind.into(),
        }
    }

    #[test]
    fn library_lookup_by_name_alias_type_and_index() {
        let libs = vec![
            lib("张三_2401110000", "user_doc_lib"),
            lib("李老师_2506190000", "shared_user_doc_lib"),
            lib("王老师_2506190001", "shared_user_doc_lib"),
            lib("北大网盘公共资源", "department_doc_lib"),
        ];
        assert_eq!(pick_lib(&libs, "个人").unwrap().name, "张三_2401110000");
        assert_eq!(
            pick_lib(&libs, "部门文档库").unwrap().name,
            "北大网盘公共资源"
        );
        assert_eq!(pick_lib(&libs, "2").unwrap().name, "李老师_2506190000");
        assert_eq!(pick_lib(&libs, "王老师").unwrap().name, "王老师_2506190001");
        assert!(pick_lib(&libs, "其他文档库").is_err()); // 两个，必须写具体名字
        assert!(pick_lib(&libs, "老师").is_err()); // 部分匹配不唯一
        assert!(pick_lib(&libs, "知识库").is_err());
    }

    #[test]
    fn sanitize_windows_names() {
        assert_eq!(sanitize("a:b?c."), "a_b_c");
        assert_eq!(sanitize("源·中乐.pdf"), "源·中乐.pdf");
    }
}
