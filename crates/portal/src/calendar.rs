//! 校历查询
//!
//! 数据源：<https://simso.pku.edu.cn/pages/ccSchoolCalendar.html>
//!
//! 这是一个 Vue SPA，校历内容被 webpack 编译到带哈希的 JS bundle 中
//! （形如 `js/ccSchoolCalendar.<hash>.js`）。每次发布哈希会变。
//!
//! 流程：
//! 1. GET 页面 HTML，正则提取 bundle 文件名；
//! 2. GET 该 JS。从 tab label 取得当前真正暴露的学年与 pane name
//!    （`label:"YYYY-YYYY学年",name:"<tag>"`），再从 Home 组件注册
//!    `components:{Calendar<tag>:<var>}` 拿到该 tab 绑定的 webpack 导出变量名；
//! 3. 顺 webpack 装配链反查 render 函数：
//!    `<var>=<mod>.exports` → `<mod>=...Object(o["a"])(<data>,<render>,…)` →
//!    `<render>=function(){var t=this,…}`。**真正的 `t._v("…")` 文字节点都
//!    在 render 函数里**，组件 data 块（`{name:"Calendar…"`)只放 props/methods。
//!    simso 的开发者经常原地复用老 .vue 文件、不改 `name:`，所以单看内部命名
//!    会错位（例如 Home 里的 `Calendar2526:g` 实际指向 `name:"Calendar2425"`
//!    的数据块，又指向新版的 render 函数 `_`）。
//! 4. render 函数体内按 `"第一学期"===t.xq?[…]` 和 `"第二学期"===t.xq?[…]`
//!    三元分支切出上 / 下学期段，再各自抽 `t._v("…")`。

use crate::client::{self, SIMSO_BASE};
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use regex::Regex;
use std::collections::BTreeSet;

const ENTRY_PATH: &str = "/pages/ccSchoolCalendar.html";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Semester {
    First,
    Second,
    All,
}

impl Semester {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "first" | "1" | "一" | "上" | "第一学期" => Ok(Self::First),
            "second" | "2" | "二" | "下" | "第二学期" => Ok(Self::Second),
            "all" | "both" => Ok(Self::All),
            other => Err(anyhow!(
                "学期参数无效：{other}（可选 first/second/all 或 1/2/上/下）"
            )),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::First => "第一学期",
            Self::Second => "第二学期",
            Self::All => "全部",
        }
    }
}

pub struct Calendar {
    pub year: String,
    pub first_semester: Vec<String>,
    pub second_semester: Vec<String>,
}

pub async fn fetch() -> Result<Vec<Calendar>> {
    let client = client::build_simple()?;

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

fn parse_bundle(js: &str) -> Result<Vec<Calendar>> {
    // tab pane: 学年 + 短 tag （如 2025-2026 / "2526"）
    let tab_re = Regex::new(r#"\{attrs:\{label:"(\d{4}-\d{4})学年",name:"(\d+)"\}\}"#)?;
    let tabs: Vec<(String, String)> = tab_re
        .captures_iter(js)
        .map(|c| (c[1].to_string(), c[2].to_string()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if tabs.is_empty() {
        return Err(anyhow!("JS bundle 中没找到学年 tab，simso 可能已改版"));
    }

    let v_re = Regex::new(r#"t\._v\("((?:\\.|[^"\\])*)"\)"#)?;
    let s1_re = Regex::new(r#""第一学期"===t\.xq\?\["#)?;
    let s2_re = Regex::new(r#""第二学期"===t\.xq\?\["#)?;
    let render_boundary_re = Regex::new(r#"\b[A-Za-z_$][A-Za-z0-9_$]*=function\(\)\{var t=this,e=t\.\$createElement"#)?;

    let mut out = Vec::new();
    for (year, tag) in &tabs {
        let Some((seg_start, seg_end)) = locate_render(js, tag, &render_boundary_re)? else {
            continue;
        };
        let segment = &js[seg_start..seg_end];

        let s1_pos = s1_re.find(segment).map(|m| m.end());
        let s2_pos = s2_re.find(segment).map(|m| m.end());

        let first_slice = match (s1_pos, s2_pos) {
            (Some(s1), Some(s2)) if s2 > s1 => Some(&segment[s1..s2]),
            (Some(s1), Some(_)) => Some(&segment[s1..]),
            (Some(s1), None) => Some(&segment[s1..]),
            _ => None,
        };
        let second_slice = s2_pos.map(|s2| &segment[s2..]);

        let first = first_slice.map(|s| extract_v(s, &v_re)).unwrap_or_default();
        let second = second_slice.map(|s| extract_v(s, &v_re)).unwrap_or_default();

        if first.is_empty() && second.is_empty() {
            continue;
        }

        out.push(Calendar {
            year: year.clone(),
            first_semester: first,
            second_semester: second,
        });
    }

    if out.is_empty() {
        return Err(anyhow!("解析 bundle 后没拿到任何学年内容，simso 可能已改版"));
    }
    Ok(out)
}

/// 根据 tab 短 tag 定位实际的 webpack **render 函数体** `[start, end)`。
///
/// 链路：Home `Calendar<tag>:<var>` → `<var>=<mod>.exports` →
/// `<mod>=...Object(o["a"])(<data>,<render>,…)` → `<render>=function(){…}`。
fn locate_render(
    js: &str,
    tag: &str,
    render_boundary_re: &Regex,
) -> Result<Option<(usize, usize)>> {
    // 1. Home: `Calendar<tag>:<var>`
    let reg = Regex::new(&format!(
        r#"Calendar{}:([A-Za-z_$][A-Za-z0-9_$]*)\b"#,
        regex::escape(tag)
    ))?;
    let Some(export_var) = reg
        .captures(js)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
    else {
        return Ok(None);
    };

    // 2. `<export_var>=<mod>.exports`
    let exp = Regex::new(&format!(
        r#"\b{}=([A-Za-z_$][A-Za-z0-9_$]*)\.exports\b"#,
        regex::escape(&export_var)
    ))?;
    let Some(mod_var) = exp
        .captures(js)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
    else {
        return Ok(None);
    };

    // 3. `<mod>=...Object(o["a"])(<data>,<render>,...)`
    let wrap = Regex::new(&format!(
        r#"\b{}=\([^)]*\),?\s*Object\(o\["a"\]\)\(\s*([A-Za-z_$][A-Za-z0-9_$]*)\s*,\s*([A-Za-z_$][A-Za-z0-9_$]*)\s*,"#,
        regex::escape(&mod_var)
    ))?;
    let render_var = if let Some(c) = wrap.captures(js) {
        c.get(2).unwrap().as_str().to_string()
    } else {
        // 退路：有些 webpack 输出不带前置 `(i("..."),…)` 的逗号表达式
        let alt = Regex::new(&format!(
            r#"\b{}=Object\(o\["a"\]\)\(\s*([A-Za-z_$][A-Za-z0-9_$]*)\s*,\s*([A-Za-z_$][A-Za-z0-9_$]*)\s*,"#,
            regex::escape(&mod_var)
        ))?;
        let Some(c) = alt.captures(js) else {
            return Ok(None);
        };
        c.get(2).unwrap().as_str().to_string()
    };

    // 4. `<render>=function(){var t=this,e=t.$createElement,...`
    let render_start_re = Regex::new(&format!(
        r#"\b{}=function\(\)\{{var t=this,e=t\.\$createElement"#,
        regex::escape(&render_var)
    ))?;
    let Some(start) = render_start_re.find(js).map(|m| m.start()) else {
        return Ok(None);
    };

    // 5. 终点：下一个 render 函数定义（任何变量名）或 EOF
    let end = render_boundary_re
        .find_iter(&js[start + 1..])
        .next()
        .map(|m| start + 1 + m.start())
        .unwrap_or(js.len());
    Ok(Some((start, end)))
}

fn extract_v(s: &str, v_re: &Regex) -> Vec<String> {
    let mut out: Vec<String> = v_re
        .captures_iter(s)
        .map(|c| unescape(&c[1]))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    out.dedup();
    out
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

pub fn render(calendars: &[Calendar], year_filter: Option<&str>, semester: Semester) {
    let mut matched = false;
    for cal in calendars {
        if let Some(y) = year_filter {
            if !cal.year.contains(y) {
                continue;
            }
        }
        matched = true;
        println!(
            "{} {} {}",
            "==".cyan(),
            format!("{} 学年校历", cal.year).bold(),
            format!("[{}]", semester.label()).dimmed()
        );
        println!();
        if matches!(semester, Semester::First | Semester::All) {
            print_semester("第一学期", &cal.first_semester);
        }
        if matches!(semester, Semester::Second | Semester::All) {
            print_semester("第二学期", &cal.second_semester);
        }
    }
    if !matched {
        eprintln!(
            "{}",
            "未找到匹配的学年，可用 `portal calendar` 看全部学年".yellow()
        );
    }
}

fn print_semester(title: &str, lines: &[String]) {
    println!("  {}", title.bold().yellow());
    if lines.is_empty() {
        println!("    {}", "(无内容)".dimmed());
    } else {
        for line in lines {
            println!("    {line}");
        }
    }
    println!();
}
