//! 子命令实现

use crate::api::{MailApi, MessageDetail, FID_DRAFTS, FID_INBOX, FID_SENT, FID_SPAM, FID_TRASH};
use crate::display::{self, Preview};
use crate::parse::{self, addr_list};
use anyhow::{anyhow, bail, Context, Result};
use colored::Colorize;
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

fn print_json(v: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(v)?);
    Ok(())
}

/// 系统文件夹的英文 / 中文叫法
fn folder_alias(s: &str) -> Option<i64> {
    let id = match s.trim().to_lowercase().as_str() {
        "inbox" | "收件箱" => FID_INBOX,
        "drafts" | "draft" | "草稿" | "草稿箱" => FID_DRAFTS,
        "sent" | "已发送" | "发件箱" => FID_SENT,
        "trash" | "deleted" | "已删除" | "回收站" => FID_TRASH,
        "spam" | "junk" | "垃圾邮件" | "垃圾箱" => FID_SPAM,
        other => return other.parse().ok(),
    };
    Some(id)
}

/// 文件夹参数 → (id, 名字)。不是系统别名或数字时按名字在 getAllFolders 里找
async fn resolve_folder(api: &mut MailApi, s: &str) -> Result<(i64, String)> {
    let folders = api.folders().await?;
    let id = match folder_alias(s) {
        Some(id) => id,
        None => {
            let exact = folders.iter().find(|f| f.name == s);
            let hits: Vec<_> = folders.iter().filter(|f| f.name.contains(s)).collect();
            match (exact, hits.as_slice()) {
                (Some(f), _) => f.id,
                (None, [f]) => f.id,
                (None, []) => bail!("没有名为「{s}」的文件夹，用 `mail folders` 查看"),
                _ => bail!("有多个文件夹名字包含「{s}」，请写完整或用 id"),
            }
        }
    };
    let name = folders
        .iter()
        .find(|f| f.id == id)
        .map(|f| f.name.clone())
        .unwrap_or_else(|| format!("文件夹 {id}"));
    Ok((id, name))
}

// ─── folders / ls / read / search ──────────────────────────────

pub async fn cmd_folders(json_out: bool) -> Result<()> {
    let mut api = MailApi::open()?;
    let folders = api.folders().await?;
    if json_out {
        return print_json(&folders);
    }
    display::print_folders(&folders);
    Ok(())
}

pub async fn cmd_ls(
    folder: &str,
    unread: bool,
    page: usize,
    limit: usize,
    json_out: bool,
) -> Result<()> {
    let page = page.max(1);
    let limit = limit.clamp(1, 100);
    let mut api = MailApi::open()?;
    let (fid, name) = resolve_folder(&mut api, folder).await?;
    let (list, total) = api.list(fid, (page - 1) * limit, limit, unread).await?;
    if json_out {
        return print_json(&json!({
            "folder": {"id": fid, "name": name},
            "page": page,
            "limit": limit,
            "total": total,
            "messages": list,
        }));
    }
    let title = if unread {
        format!("{name}（未读）")
    } else {
        name
    };
    display::print_messages(&title, &list, total, page, limit);
    Ok(())
}

pub async fn cmd_read(mid: &str, html: bool, mark_read: bool, json_out: bool) -> Result<()> {
    let mut api = MailApi::open()?;
    let v = api.read(mid, mark_read).await?;
    let detail = MessageDetail::from_value(mid, &v, html);
    if json_out {
        return print_json(&detail);
    }
    display::print_message(&detail, html);
    Ok(())
}

pub async fn cmd_search(
    words: &[String],
    folder: Option<&str>,
    page: usize,
    json_out: bool,
) -> Result<()> {
    const WINDOW: usize = 20;
    let pattern = words.join(" ");
    if pattern.trim().is_empty() {
        bail!("搜索词不能为空");
    }
    let page = page.max(1);
    let mut api = MailApi::open()?;
    let fids = match folder {
        Some(f) => vec![resolve_folder(&mut api, f).await?.0],
        None => Vec::new(),
    };
    let (list, total) = api
        .search(pattern.trim(), &fids, (page - 1) * WINDOW, WINDOW)
        .await?;
    if json_out {
        return print_json(&json!({
            "pattern": pattern,
            "page": page,
            "total": total,
            "messages": list,
        }));
    }
    display::print_messages(&format!("搜索「{pattern}」"), &list, total, page, WINDOW);
    Ok(())
}

// ─── download ──────────────────────────────────────────────────

pub async fn cmd_download(
    mid: &str,
    part: Option<&str>,
    output: Option<&Path>,
    json_out: bool,
) -> Result<()> {
    let mut api = MailApi::open()?;
    let detail = MessageDetail::from_value(mid, &api.read(mid, false).await?, false);
    let targets: Vec<_> = match part {
        Some(p) => {
            let a = detail
                .attachments
                .iter()
                .find(|a| a.id == p)
                .cloned()
                .ok_or_else(|| {
                    anyhow!(
                        "这封邮件没有附件 {p}（有：{}）",
                        detail
                            .attachments
                            .iter()
                            .map(|a| a.id.as_str())
                            .collect::<Vec<_>>()
                            .join("、")
                    )
                })?;
            vec![a]
        }
        None => detail.attachments.clone(),
    };
    if targets.is_empty() {
        bail!("这封邮件没有附件");
    }
    let out = output.map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    tokio::fs::create_dir_all(&out)
        .await
        .with_context(|| format!("创建目录 {} 失败", out.display()))?;

    let mut saved = Vec::new();
    for a in &targets {
        let mut resp = api.attachment(mid, &a.id).await?;
        let header_name = resp
            .headers()
            .get("content-disposition")
            .map(|v| String::from_utf8_lossy(v.as_bytes()).into_owned())
            .and_then(|h| parse::disposition_filename(&h));
        let name = parse::sanitize_filename(&header_name.unwrap_or_else(|| a.filename.clone()));
        let dest = out.join(&name);
        let tmp = out.join(format!("{name}.part"));
        let mut file = tokio::fs::File::create(&tmp)
            .await
            .with_context(|| format!("创建 {} 失败", tmp.display()))?;
        let mut bytes = 0u64;
        while let Some(chunk) = resp.chunk().await.context("下载附件中断")? {
            file.write_all(&chunk)
                .await
                .with_context(|| format!("写入 {} 失败", tmp.display()))?;
            bytes += chunk.len() as u64;
        }
        file.flush().await?;
        drop(file);
        tokio::fs::rename(&tmp, &dest)
            .await
            .with_context(|| format!("保存 {} 失败", dest.display()))?;
        if !json_out {
            println!(
                "  {} {} {}",
                "↓".green(),
                dest.display(),
                display::fmt_size(bytes as i64).dimmed()
            );
        }
        saved.push(json!({"part": a.id, "path": dest, "size": bytes}));
    }
    if json_out {
        return print_json(&saved);
    }
    println!(
        "{} 已下载 {} 个附件 → {}",
        "✓".green(),
        saved.len(),
        out.display()
    );
    Ok(())
}

// ─── contacts ──────────────────────────────────────────────────

pub async fn cmd_contacts(query: Option<&str>, json_out: bool) -> Result<()> {
    let mut api = MailApi::open()?;
    let mut list = api.contacts().await?;
    if let Some(q) = query.map(str::trim).filter(|q| !q.is_empty()) {
        list.retain(|c| c.matches(q));
    }
    if json_out {
        return print_json(&list);
    }
    display::print_contacts(&list);
    Ok(())
}

// ─── send / reply ──────────────────────────────────────────────

/// 读正文：`--body` 或 `--body-file`（`-` 表示标准输入）
pub fn read_body(body: Option<&str>, body_file: Option<&Path>) -> Result<String> {
    let text = match (body, body_file) {
        (Some(b), _) => b.to_string(),
        (None, Some(p)) if p == Path::new("-") => {
            let mut s = String::new();
            std::io::stdin()
                .read_to_string(&mut s)
                .context("从标准输入读取正文失败")?;
            s
        }
        (None, Some(p)) => std::fs::read_to_string(p)
            .with_context(|| format!("读取正文文件 {} 失败", p.display()))?,
        (None, None) => bail!("需要 --body 或 --body-file"),
    };
    if text.trim().is_empty() {
        bail!("正文不能为空");
    }
    Ok(text)
}

fn body_html(text: &str, is_html: bool) -> String {
    if is_html {
        text.to_string()
    } else {
        parse::text_to_html(text)
    }
}

fn not_sent_hint(json_out: bool) {
    if !json_out {
        println!(
            "{}",
            "未发送。确认收件人、主题和正文无误后，加 --yes 重新运行才会真正发送（发出后无法撤回）。"
                .yellow()
        );
    }
}

pub struct SendArgs<'a> {
    pub to: &'a [String],
    pub cc: &'a [String],
    pub bcc: &'a [String],
    pub subject: &'a str,
    pub body: String,
    pub html: bool,
    pub yes: bool,
}

pub async fn cmd_send(args: SendArgs<'_>, json_out: bool) -> Result<()> {
    let to = parse::normalize_recipients(args.to)?;
    let cc = parse::normalize_recipients(args.cc)?;
    let bcc = parse::normalize_recipients(args.bcc)?;
    if to.is_empty() {
        bail!("至少需要一个收件人（--to）");
    }
    let subject = args.subject.trim();
    if subject.is_empty() {
        bail!("主题不能为空");
    }
    let mut api = MailApi::open()?;
    let account = api.sess.account.clone();
    let content = body_html(&args.body, args.html);
    let shown_body = if args.html {
        parse::html_to_text(&args.body)
    } else {
        args.body.clone()
    };

    if !args.yes {
        if json_out {
            return print_json(&json!({
                "sent": false,
                "preview": {
                    "from": account, "to": to, "cc": cc, "bcc": bcc,
                    "subject": subject, "body": shown_body,
                },
                "hint": "加 --yes 才会真正发送",
            }));
        }
        display::print_preview(&Preview {
            kind: "邮件",
            from: &account,
            to: &to,
            cc: &cc,
            bcc: &bcc,
            subject,
            body: &shown_body,
            note: None,
        });
        not_sent_hint(json_out);
        return Ok(());
    }

    let result = api
        .compose(
            None,
            json!({
                "account": account,
                "to": to,
                "cc": cc,
                "bcc": bcc,
                "subject": subject,
                "content": content,
                "isHtml": true,
                "saveSentCopy": true,
            }),
        )
        .await?;
    if json_out {
        return print_json(&json!({"sent": true, "result": result}));
    }
    println!(
        "{} 已发送给 {}（副本在「已发送」）",
        "✓".green(),
        to.join(", ")
    );
    Ok(())
}

pub async fn cmd_reply(
    mid: &str,
    all: bool,
    body: String,
    html: bool,
    yes: bool,
    json_out: bool,
) -> Result<()> {
    let mut api = MailApi::open()?;
    let draft = api.reply_prepare(mid, all).await?;
    let compose_id = match draft.get("id") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => bail!(
            "服务器没有返回回复草稿 id：{}",
            parse::snippet(&draft.to_string())
        ),
    };
    let to = addr_list(draft.get("to"));
    let cc = addr_list(draft.get("cc"));
    if to.is_empty() {
        bail!("回复草稿没有收件人");
    }
    let subject = draft
        .get("subject")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let quoted = draft
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let account = draft
        .get("account")
        .and_then(Value::as_str)
        .filter(|s| s.contains('@'))
        .map_or_else(|| api.sess.account.clone(), str::to_string);
    let shown_body = if html {
        parse::html_to_text(&body)
    } else {
        body.clone()
    };

    if !yes {
        if json_out {
            return print_json(&json!({
                "sent": false,
                "preview": {
                    "from": account, "to": to, "cc": cc,
                    "subject": subject, "body": shown_body,
                    "quoted_original": parse::html_to_text(&quoted),
                },
                "hint": "加 --yes 才会真正发送",
            }));
        }
        display::print_preview(&Preview {
            kind: if all { "回复全部" } else { "回复" },
            from: &account,
            to: &to,
            cc: &cc,
            bcc: &[],
            subject: &subject,
            body: &shown_body,
            note: (!quoted.is_empty()).then_some("（下方附带原邮件引用）"),
        });
        not_sent_hint(json_out);
        return Ok(());
    }

    let mut attrs = match draft {
        Value::Object(map) => map,
        _ => bail!("回复草稿格式异常"),
    };
    attrs.remove("id");
    attrs.insert("account".into(), json!(account));
    attrs.insert(
        "content".into(),
        json!(format!("{}<br>{quoted}", body_html(&body, html))),
    );
    attrs.insert("isHtml".into(), json!(true));
    attrs.insert("saveSentCopy".into(), json!(true));
    let result = api.compose(Some(&compose_id), Value::Object(attrs)).await?;
    if json_out {
        return print_json(&json!({"sent": true, "result": result}));
    }
    println!("{} 已回复 {}", "✓".green(), to.join(", "));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_aliases() {
        assert_eq!(folder_alias("inbox"), Some(1));
        assert_eq!(folder_alias("已发送"), Some(3));
        assert_eq!(folder_alias("Trash"), Some(4));
        assert_eq!(folder_alias("-1"), Some(-1));
        assert_eq!(folder_alias("18"), Some(18));
        assert_eq!(folder_alias("订阅邮件"), None);
    }
}
