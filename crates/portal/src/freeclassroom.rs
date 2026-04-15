//! 空闲教室查询
//!
//! 接口：GET `/publicQuery/classroomQuery/retrClassRoomFree.do?buildingName=<中文>&time=<中文>`
//!
//! 无需登录。响应 JSON：
//! ```json
//! {"success":true,"results":25,"rows":[{"room":"101","cap":"221","c1":"占用",...,"c12":""}]}
//! ```
//! `c1`..`c12` 是 12 节课，"占用" 表示被占用，空串表示空闲。

use crate::client::{self, PORTAL_PUBLIC_BASE};
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use serde::Deserialize;

pub const BUILDINGS: &[&str] = &[
    "一教", "二教", "三教", "四教", "理教", "文史", "哲学", "地学", "国关", "政管",
];

/// 查询日期
#[derive(Debug, Clone, Copy)]
pub enum Day {
    Today,
    Tomorrow,
    DayAfter,
}

impl Day {
    pub fn to_query(self) -> &'static str {
        match self {
            Day::Today => "今天",
            Day::Tomorrow => "明天",
            Day::DayAfter => "后天",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "today" | "今天" | "0" => Day::Today,
            "tomorrow" | "明天" | "1" => Day::Tomorrow,
            "day-after" | "dayafter" | "后天" | "2" => Day::DayAfter,
            _ => return Err(anyhow!("未知日期: {s}（可选 today/tomorrow/day-after）")),
        })
    }
}

#[derive(Debug, Deserialize)]
struct Resp {
    success: bool,
    rows: Vec<Row>,
}

#[derive(Debug, Deserialize)]
pub struct Row {
    pub room: String,
    pub cap: String,
    #[serde(default)]
    pub c1: String,
    #[serde(default)]
    pub c2: String,
    #[serde(default)]
    pub c3: String,
    #[serde(default)]
    pub c4: String,
    #[serde(default)]
    pub c5: String,
    #[serde(default)]
    pub c6: String,
    #[serde(default)]
    pub c7: String,
    #[serde(default)]
    pub c8: String,
    #[serde(default)]
    pub c9: String,
    #[serde(default)]
    pub c10: String,
    #[serde(default)]
    pub c11: String,
    #[serde(default)]
    pub c12: String,
}

impl Row {
    /// 12 节是否占用
    pub fn slots(&self) -> [bool; 12] {
        let s = [
            &self.c1, &self.c2, &self.c3, &self.c4, &self.c5, &self.c6, &self.c7, &self.c8,
            &self.c9, &self.c10, &self.c11, &self.c12,
        ];
        let mut out = [false; 12];
        for (i, v) in s.iter().enumerate() {
            out[i] = !v.is_empty();
        }
        out
    }
}

pub async fn query(building: &str, day: Day) -> Result<Vec<Row>> {
    let building = normalize_building(building)?;
    let client = client::build_simple()?;
    let url = format!(
        "{PORTAL_PUBLIC_BASE}/classroomQuery/retrClassRoomFree.do?buildingName={}&time={}",
        urlencoding::encode(building),
        urlencoding::encode(day.to_query()),
    );
    let resp: Resp = client
        .get(&url)
        .header("accept", "application/json, text/plain, */*")
        .header("referer", "https://portal.pku.edu.cn/publicQuery/")
        .send()
        .await
        .context("空闲教室查询失败")?
        .json()
        .await
        .context("空闲教室响应解析失败")?;
    if !resp.success {
        return Err(anyhow!("空闲教室查询未成功"));
    }
    Ok(resp.rows)
}

fn normalize_building(input: &str) -> Result<&'static str> {
    for b in BUILDINGS {
        if *b == input {
            return Ok(b);
        }
    }
    // 支持数字/英文别名
    Ok(match input {
        "1" | "1教" => "一教",
        "2" | "2教" => "二教",
        "3" | "3教" => "三教",
        "4" | "4教" => "四教",
        "li" | "理" => "理教",
        "ws" | "文" => "文史",
        "zx" | "哲" => "哲学",
        "dx" | "地" => "地学",
        "gg" | "国" => "国关",
        "zg" | "政" => "政管",
        _ => {
            return Err(anyhow!(
                "未知教学楼 '{input}'。支持：{}",
                BUILDINGS.join("、")
            ))
        }
    })
}

/// 终端渲染空闲教室
pub fn render(building: &str, day: Day, rows: &[Row]) {
    println!(
        "{} {} ({})",
        "空闲教室".bold().cyan(),
        building,
        day.to_query()
    );
    println!();
    print!("  {:<6}{:<6}", "教室", "座位");
    for i in 1..=12 {
        print!("{i:>3}");
    }
    println!();
    for row in rows {
        print!("  {:<6}{:<6}", row.room, row.cap);
        for occupied in row.slots() {
            if occupied {
                print!("  {}", "×".red());
            } else {
                print!("  {}", "·".green());
            }
        }
        println!();
    }
    println!();
    println!("  {} 空闲   {} 占用", "·".green(), "×".red());
}
