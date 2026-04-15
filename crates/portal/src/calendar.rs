//! 校历查询
//!
//! 数据源：<https://simso.pku.edu.cn/pages/ccSchoolCalendar.html>
//!
//! 这是一个 Vue SPA，校历内容被 webpack 编译到带哈希的 JS bundle 中
//! （形如 `js/ccSchoolCalendar.<hash>.js`）。每次发布哈希会变。
//!
//! 因此流程分两步：
//! 1. GET 页面 HTML，正则提取 `ccSchoolCalendar.<hash>.js` 的文件名；
//! 2. GET 该 JS，正则抽取 Vue 编译产物里的 `_v("...")` 文本节点，
//!    这些即是校历中的每一条文字。
//!
//! 这个方案是 best-effort——若 simso 改版为真·API 或改变编译方式，
//! 正则会失效。此时用户可以直接访问原网页。

use crate::client::{self, SIMSO_BASE};
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use regex::Regex;

const ENTRY_PATH: &str = "/pages/ccSchoolCalendar.html";

pub struct Calendar {
    pub year: String,
    pub lines: Vec<String>,
}

pub async fn fetch() -> Result<Vec<Calendar>> {
    let client = client::build_simple()?;

    // 1. 页面 HTML
    let html = client
        .get(format!("{SIMSO_BASE}{ENTRY_PATH}"))
        .send()
        .await
        .context("访问 simso 校历页面失败")?
        .text()
        .await?;

    let js_rel = Regex::new(r#"(js/ccSchoolCalendar\.[0-9a-f]+\.js)"#)?
        .captures(&html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| anyhow!("未在页面中找到校历 JS bundle 文件名，simso 可能已改版"))?;

    // 2. JS bundle
    let js_url = format!("{SIMSO_BASE}/{js_rel}");
    let js = client
        .get(&js_url)
        .send()
        .await
        .context("下载校历 JS bundle 失败")?
        .text()
        .await?;

    parse_bundle(&js)
}

/// 从 Vue 编译产物中抽取按学年分组的文字节点
fn parse_bundle(js: &str) -> Result<Vec<Calendar>> {
    // 每个 `Calendar25xx` 组件是一个独立函数。学年标签可从 tab label 里找到：
    // `i("el-tab-pane",{attrs:{label:"2025-2026学年",name:"2526"}`
    let year_re = Regex::new(r#"label:"(\d{4}-\d{4})学年","#)?;
    let years: Vec<String> = year_re
        .captures_iter(js)
        .map(|c| c[1].to_string())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    // 每个学年组件的文本节点都用 `t._v("...")` 的形式编译（t 是 this）。
    // Vue 会把每个 StaticText 转成这样一条调用，所以我们扫全部文件中的 _v 参数。
    // 简化：不再按学年细分，而是返回所有学年各自一份全量文本列表。
    // 如果需要更细的学年分割，可以按 `Calendar2526` / `Calendar2627` 组件名切分 JS。
    let v_re = Regex::new(r#"t\._v\("((?:\\.|[^"\\])*)"\)"#)?;
    let mut all_lines: Vec<String> = v_re
        .captures_iter(js)
        .map(|c| unescape(&c[1]))
        .filter(|s| !s.trim().is_empty())
        .collect();
    all_lines.dedup();

    // 按学年组件切分：使用正则定位每个 `Calendar2526` 函数定义的起止区间
    let comp_re = Regex::new(r#"Calendar(\d{4})"#)?;
    let comp_positions: Vec<(usize, String)> = comp_re
        .captures_iter(js)
        .map(|c| (c.get(0).unwrap().start(), c[1].to_string()))
        .collect::<std::collections::BTreeMap<_, _>>()
        .into_iter()
        .collect();

    if comp_positions.is_empty() || years.is_empty() {
        // 退回全量输出
        return Ok(vec![Calendar {
            year: "校历".to_string(),
            lines: all_lines,
        }]);
    }

    let mut out = Vec::new();
    for (i, (start, tag)) in comp_positions.iter().enumerate() {
        let end = comp_positions
            .get(i + 1)
            .map(|(e, _)| *e)
            .unwrap_or(js.len());
        let segment = &js[*start..end];
        let lines: Vec<String> = v_re
            .captures_iter(segment)
            .map(|c| unescape(&c[1]))
            .filter(|s| !s.trim().is_empty())
            .collect();
        if lines.is_empty() {
            continue;
        }
        let year = match tag.as_str() {
            "2526" => "2025-2026".to_string(),
            "2627" => "2026-2027".to_string(),
            other => format!("20{}-20{}", &other[..2], &other[2..]),
        };
        out.push(Calendar { year, lines });
    }

    if out.is_empty() {
        out.push(Calendar {
            year: "校历".to_string(),
            lines: all_lines,
        });
    }

    Ok(out)
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                match next {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    '"' | '\\' | '/' => out.push(next),
                    _ => {
                        out.push('\\');
                        out.push(next);
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn render(calendars: &[Calendar], year_filter: Option<&str>) {
    for cal in calendars {
        if let Some(y) = year_filter {
            if !cal.year.contains(y) {
                continue;
            }
        }
        println!(
            "{} {}",
            "==".cyan(),
            format!("{} 学年校历", cal.year).bold()
        );
        println!();
        for line in &cal.lines {
            println!("  {line}");
        }
        println!();
    }
}
