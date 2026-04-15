//! 北大空间 API 客户端

use crate::client::BDKJ_BASE;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

/// 教学楼 ID（从页面硬编码拿到）
pub const BUILDING_ERJIAO: &str = "924128798427975680"; // 二教
pub const BUILDING_SIJIAO: &str = "924128697978589184"; // 四教
pub const BUILDING_DIXUE: &str = "924129024664539136"; // 地学

/// 根据名字解析教学楼 ID
pub fn building_id(name: &str) -> Option<&'static str> {
    match name {
        "二教" | "erjiao" | "02" => Some(BUILDING_ERJIAO),
        "四教" | "sijiao" | "04" => Some(BUILDING_SIJIAO),
        "地学" | "dixue" | "09" => Some(BUILDING_DIXUE),
        _ => None,
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Room {
    pub id: String,
    #[serde(default)]
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub popularity: String,
    #[serde(rename = "seatingCapacity", default)]
    pub seating_capacity: String,
    #[serde(default)]
    pub bookable: bool,
    #[serde(default)]
    pub locked: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HistoryTime {
    pub id: String,
    #[serde(rename = "beginTime")]
    pub begin_time: String,
    #[serde(rename = "endTime")]
    pub end_time: String,
    #[serde(default)]
    pub intervals: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchStudentResp {
    pub success: bool,
    #[serde(default)]
    pub message: String,
    pub data: Option<StudentInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StudentInfo {
    pub id: String,
    pub serial: String,
    pub name: String,
    #[serde(default)]
    pub college: Option<String>,
    #[serde(rename = "mobilePhone", default)]
    pub mobile_phone: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Participant {
    pub student_id: String,
    pub serial: String,
    pub name: String,
}

/// 一条已有的预约申请（从 /classRoom 页面抽取）
#[derive(Debug, Clone, Serialize)]
pub struct Application {
    pub id: String,
    pub room_name: String,
    pub status: String,
    pub applicant: String,
    pub apply_time: String,
    pub begin_end: String,
    pub reason: String,
    pub participants: Vec<String>,
    pub can_cancel: bool,
}

pub struct BdkjApi {
    http: reqwest::Client,
}

impl BdkjApi {
    pub fn new(http: reqwest::Client) -> Self {
        Self { http }
    }

    /// 按教学楼列出可预约的教室
    pub async fn list_rooms(&self, building_id: &str) -> Result<Vec<Room>> {
        let resp = self
            .http
            .post(format!("{BDKJ_BASE}/room/classRoom"))
            .header("x-requested-with", "XMLHttpRequest")
            .header("accept", "application/json, text/javascript, */*; q=0.01")
            .form(&[
                ("buildingId", building_id),
                ("roomId", ""),
                ("beginTime", ""),
                ("endTime", ""),
            ])
            .send()
            .await
            .context("列出教室请求失败")?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            return Err(anyhow!("列出教室 HTTP {status}: {body}"));
        }
        let rooms: Vec<Room> =
            serde_json::from_str(&body).with_context(|| format!("解析教室列表失败: {body}"))?;
        Ok(rooms)
    }

    /// 查询指定教室已被预约的时间段
    pub async fn history_time(&self, room_id: &str, start_date: &str) -> Result<Vec<HistoryTime>> {
        let resp = self
            .http
            .post(format!(
                "{BDKJ_BASE}/classRoom/historyTime?id=&roomId={room_id}"
            ))
            .header("x-requested-with", "XMLHttpRequest")
            .header("accept", "application/json, text/javascript, */*; q=0.01")
            .form(&[("startDate", start_date)])
            .send()
            .await
            .context("查询已预约时段请求失败")?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            return Err(anyhow!("historyTime HTTP {status}: {body}"));
        }
        let list: Vec<HistoryTime> =
            serde_json::from_str(&body).with_context(|| format!("解析历史时段失败: {body}"))?;
        Ok(list)
    }

    /// 根据学号+姓名查询学生信息
    pub async fn search_student(&self, serial: &str, name: &str) -> Result<StudentInfo> {
        let resp = self
            .http
            .post(format!("{BDKJ_BASE}/classRoom/seachStudent"))
            .header("x-requested-with", "XMLHttpRequest")
            .header("accept", "application/json, text/javascript, */*; q=0.01")
            .form(&[("serial", serial), ("name", name)])
            .send()
            .await
            .context("查询学生请求失败")?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            return Err(anyhow!("seachStudent HTTP {status}: {body}"));
        }
        let parsed: SearchStudentResp =
            serde_json::from_str(&body).with_context(|| format!("解析学生信息失败: {body}"))?;
        if !parsed.success {
            return Err(anyhow!("seachStudent 失败: {}", parsed.message));
        }
        parsed
            .data
            .ok_or_else(|| anyhow!("seachStudent 返回 data 为空"))
    }

    /// 提交预约
    ///
    /// `begin_time` / `end_time` 格式 `YYYY-MM-DD HH:MM:SS`
    pub async fn submit_apply(
        &self,
        room_id: &str,
        begin_time: &str,
        end_time: &str,
        reason: &str,
        participants: &[Participant],
    ) -> Result<SubmitResult> {
        // 表单字段与浏览器 step4 提交完全一致
        let mut form: Vec<(String, String)> = vec![
            ("id".into(), "".into()),
            ("beginTime".into(), begin_time.into()),
            ("endTime".into(), end_time.into()),
            ("roomId".into(), room_id.into()),
            ("step".into(), "3".into()),
            ("switchover".into(), "true".into()),
            ("reason".into(), reason.into()),
        ];
        for (i, p) in participants.iter().enumerate() {
            form.push((format!("students[{i}].studentId"), p.student_id.clone()));
            form.push((format!("students[{i}].serial"), p.serial.clone()));
            form.push((format!("students[{i}].name"), p.name.clone()));
            form.push((format!("students[{i}].sort"), i.to_string()));
        }

        let resp = self
            .http
            .post(format!("{BDKJ_BASE}/classRoom/handle/submit"))
            .header("referer", format!("{BDKJ_BASE}/classRoom/apply"))
            .form(&form)
            .send()
            .await
            .context("提交预约请求失败")?;
        let status = resp.status();
        let final_url = resp.url().clone();
        let body = resp.text().await?;

        if !status.is_success() {
            return Err(anyhow!("submit HTTP {status}: {body}"));
        }
        // 成功：服务器 302 → /classRoom（列表页包含 `申请成功`/`申请已取消` 等状态）
        // 失败：停在 /classRoom/apply，同时页面里有一条 layer.msg(...) 展示报错
        let path = final_url.path();
        let landed_on_list = path == "/classRoom" || path.ends_with("/classRoom");
        if landed_on_list {
            return Ok(SubmitResult {
                success: true,
                message: "预约提交成功".into(),
                landed_url: final_url.to_string(),
            });
        }
        let msg = extract_layer_msg(&body).unwrap_or_else(|| format!("未知返回: path={path}"));
        Ok(SubmitResult {
            success: false,
            message: msg,
            landed_url: final_url.to_string(),
        })
    }

    /// 取消预约
    pub async fn cancel_apply(&self, apply_id: &str) -> Result<()> {
        let resp = self
            .http
            .get(format!("{BDKJ_BASE}/classRoom/cancelApply/{apply_id}"))
            .send()
            .await
            .context("取消预约请求失败")?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            return Err(anyhow!("cancelApply HTTP {status}: {body}"));
        }
        if let Some(msg) = extract_layer_msg(&body) {
            if !msg.contains("成功") && !msg.contains("取消成功") {
                return Err(anyhow!("cancelApply 返回: {msg}"));
            }
        }
        Ok(())
    }

    /// 拉取 /classRoom 主页面并抽取申请列表
    pub async fn list_applications(&self) -> Result<Vec<Application>> {
        let resp = self
            .http
            .get(format!("{BDKJ_BASE}/classRoom"))
            .send()
            .await
            .context("获取 classRoom 页面失败")?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            return Err(anyhow!("classRoom HTTP {status}"));
        }
        Ok(parse_applications(&body))
    }
}

#[derive(Debug)]
pub struct SubmitResult {
    pub success: bool,
    pub message: String,
    pub landed_url: String,
}

/// 从 HTML 中抠出 `layer.msg("...")` 的文字
fn extract_layer_msg(html: &str) -> Option<String> {
    let re = regex::Regex::new(r#"layer\.msg\(["']([^"']+)["']"#).ok()?;
    re.captures(html)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
}

/// 解析 /classRoom 页面中的申请列表
///
/// 页面结构：`<div id="results"> <div class="row">…每条申请…</div> </div>`
fn parse_applications(html: &str) -> Vec<Application> {
    use scraper::{Html, Selector};
    let doc = Html::parse_document(html);
    let row_sel = Selector::parse("div#results > div.row").unwrap();
    let h5_sel = Selector::parse("h5").unwrap();

    let cancel_re = regex::Regex::new(r"/classRoom/cancelApply/(\d+)").unwrap();
    let begin_end_re =
        regex::Regex::new(r"(\d{4}-\d{2}-\d{2} \d{2}:\d{2}\s*-\s*\d{2}:\d{2})").unwrap();
    let apply_time_re = regex::Regex::new(r"(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})").unwrap();
    let applicant_re = regex::Regex::new(r"([^\s]+)\s+申请$").unwrap();
    let reason_re = regex::Regex::new(r"(?s)fa-comment-dots[^<]*</em>\s*([^<]+)").unwrap();
    let status_re = regex::Regex::new(r#"(?s)color-red"><strong>([^<]+)"#).unwrap();

    let mut out = Vec::new();
    for node in doc.select(&row_sel) {
        let html_s = node.html();
        let text = node.text().collect::<Vec<_>>().join(" ");
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");

        let id = cancel_re
            .captures(&html_s)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let can_cancel = !id.is_empty();

        let room_name = node
            .select(&h5_sel)
            .next()
            .map(|n| n.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        if room_name.is_empty() {
            continue;
        }

        let status_raw = status_re
            .captures(&html_s)
            .and_then(|c| c.get(1).map(|m| m.as_str().trim().to_string()))
            .unwrap_or_default();
        let status = if status_raw.contains("申请成功") {
            "申请成功".into()
        } else if status_raw.contains("已取消") {
            "申请已取消".into()
        } else if status_raw.contains("已结束") {
            "已结束".into()
        } else if !status_raw.is_empty() {
            status_raw
        } else {
            "未知".into()
        };

        let apply_time = apply_time_re
            .captures(&text)
            .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
            .unwrap_or_default();
        let begin_end = begin_end_re
            .captures(&text)
            .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
            .unwrap_or_default();
        // 申请人在 "xxx 申请 <time>" 的前面
        let applicant = text
            .split_once(&apply_time)
            .map(|(prefix, _)| {
                applicant_re
                    .captures(prefix.trim_end())
                    .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        let reason = reason_re
            .captures(&html_s)
            .and_then(|c| c.get(1).map(|m| m.as_str().trim().to_string()))
            .unwrap_or_default();

        // 参与人列表：文本形如 "张三[ 未签到 ] 李四[ ... ]"
        let participants = text
            .split(']')
            .filter_map(|seg| {
                seg.rsplit_once('[')
                    .map(|(left, _)| left.trim().to_string())
            })
            .filter(|n| !n.is_empty() && n != "申请")
            .collect::<Vec<_>>();

        out.push(Application {
            id,
            room_name,
            status,
            applicant,
            apply_time,
            begin_end,
            reason,
            participants,
            can_cancel,
        });
    }
    out
}
