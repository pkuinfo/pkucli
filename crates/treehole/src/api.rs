//! 树洞 API 客户端 — 封装所有 HTTP 请求

use crate::client::{self, TREEHOLE_BASE};
use anyhow::{anyhow, Context, Result};
use info_common::session::Store;
use reqwest::Client;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

const APP_NAME: &str = "treehole";

// ─── 通用响应结构 ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ApiResp<T> {
    pub code: i64,
    pub success: bool,
    pub message: String,
    #[serde(default)]
    pub data: Option<T>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ListData<T> {
    pub list: Vec<T>,
}

// ─── 数据模型 ───────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
pub struct Hole {
    pub pid: i64,
    pub text: String,
    pub timestamp: i64,
    #[serde(default)]
    pub reply: i64,
    #[serde(default)]
    pub likenum: i64,
    #[serde(default)]
    pub tread_num: i64,
    #[serde(default)]
    pub is_follow: i64,
    #[serde(default)]
    pub reward_cost: i64,
    #[serde(default)]
    pub tags_info: Vec<TagInfo>,
    #[serde(default)]
    pub is_top: i64,
}

#[derive(Debug, Default)]
pub struct HoleWithComments {
    pub hole: Hole,
    pub list: Vec<Comment>,
    pub total: Option<i64>,
}

/// list_comments 返回的帖子（带内嵌评论）
///
/// 不使用 #[serde(flatten)] 避免字段类型冲突（attention_info 有时是数组有时是对象）
#[derive(Debug, Default, Deserialize)]
pub struct HoleListItem {
    #[serde(default)]
    pub pid: i64,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub reply: i64,
    #[serde(default)]
    pub likenum: i64,
    #[serde(default)]
    pub tread_num: i64,
    #[serde(default)]
    pub is_follow: i64,
    #[serde(default)]
    pub is_top: i64,
    #[serde(default)]
    pub reward_cost: i64,
    #[serde(default)]
    pub tags_info: Vec<TagInfo>,
    #[serde(default, alias = "comments")]
    pub comment_list: Vec<Comment>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Comment {
    pub cid: i64,
    pub text: String,
    pub timestamp: i64,
    #[serde(default)]
    pub name_tag: String,
    #[serde(default)]
    pub is_lz: i64,
    #[serde(default)]
    pub quote: serde_json::Value,
}

#[derive(Debug, Default, Deserialize)]
pub struct TagInfo {
    #[serde(default)]
    pub tag_name: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct UserInfo {
    #[serde(default)]
    pub uid: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub newmsgcount: i64,
    #[serde(default)]
    pub action_remaining: i64,
    #[serde(default)]
    pub leaf_balance: i64,
    #[serde(default)]
    pub is_black: i64,
}

#[derive(Debug, Default, Deserialize)]
pub struct Message {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub pid: Option<i64>,
    #[serde(default)]
    pub is_read: i64,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct UnreadCount {
    #[serde(default)]
    pub count: i64,
}

#[derive(Debug, Default, Deserialize)]
pub struct Bookmark {
    #[serde(default)]
    pub id: i64,
}

// ─── 成绩相关 ─────────────────────────────────────────────────────

/// 成绩查询 - 单门课程
#[derive(Debug, Clone, Deserialize)]
pub struct CourseScore {
    #[serde(default)]
    pub kcmc: String,        // 课程名称
    #[serde(default)]
    pub xf: String,          // 学分
    #[serde(default)]
    pub xqcj: String,        // 成绩
    #[serde(default)]
    pub kclbmc: String,      // 课程类别名称 (全校任选/专业必修/...)
    #[serde(default)]
    pub xnd: String,         // 学年度 e.g. "25-26"
    #[serde(default)]
    pub xq: String,          // 学期 "1" or "2" or "3"
}

/// 学期GPA
#[derive(Debug, Clone, Deserialize)]
pub struct SemesterGpa {
    pub gpa: String,
    pub xndxq: String,       // e.g. "25-26-1"
}

/// 成绩查询总响应
#[derive(Debug)]
pub struct ScoreData {
    pub courses: Vec<CourseScore>,
    pub semester_gpas: Vec<SemesterGpa>,
    pub overall_gpa: String,
    pub total_credits: String,
}

// ─── 课表相关 ─────────────────────────────────────────────────────

/// 课表中的一个时间段
#[derive(Debug, Clone)]
pub struct CourseSlot {
    pub course_name: String,
    pub style: String,        // background-color
}

/// 课表一行（一节课在一周中的分布）
#[derive(Debug, Clone)]
pub struct CourseRow {
    pub time_num: String,     // "第一节" etc
    pub slots: [Option<CourseSlot>; 7], // mon..sun
}

/// 作息时间
#[derive(Debug, Clone, Deserialize)]
pub struct ClassTime {
    pub name: String,
    #[serde(default)]
    pub time_period: String,
}

// ─── 学术日历 ─────────────────────────────────────────────────────

/// 学术日历事件
#[derive(Debug, Clone, Deserialize)]
pub struct LabEvent {
    #[serde(default, alias = "TITLE")]
    pub title: String,
    #[serde(default, alias = "DEPT")]
    pub dept: String,
    #[serde(default, alias = "LOCATION")]
    pub location: Option<String>,
    #[serde(default, alias = "HOST")]
    pub host: Option<String>,
    #[serde(default, alias = "START_TIME")]
    pub start_time: String,
}

// ─── 活动日历 ─────────────────────────────────────────────────────

/// 活动日历事件
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ActivityEvent {
    #[serde(default)]
    pub event_name: String,
    #[serde(default)]
    pub event_start_time: String,
    #[serde(default)]
    pub event_location: String,
    #[serde(default)]
    pub event_organizer: String,
    #[serde(default)]
    pub event_introduction: String,
    #[serde(default)]
    pub event_type_name: String,
}

// ─── 周日程 ───────────────────────────────────────────────────────

/// 周日程
#[derive(Debug, Clone, Deserialize)]
pub struct ScheduleItem {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub start_time: String,
    #[serde(default)]
    pub end_time: String,
}

// ─── API Client ─────────────────────────────────────────────────

pub struct TreeholeApi {
    client: Client,
    token: String,
    uuid: String,
}

impl TreeholeApi {
    /// 从本地 session 加载并确保短信验证通过（如需要会交互式引导完成）
    pub async fn from_session_verified() -> Result<Self> {
        let store = Store::new(APP_NAME)?;
        let session = store
            .load_session()?
            .ok_or_else(|| anyhow!("未登录，请先运行 `treehole login`"))?;

        if session.is_expired() {
            return Err(anyhow!("会话已过期，请重新运行 `treehole login`"));
        }

        let uuid = session
            .extra
            .get("full_uuid")
            .and_then(|v| v.as_str())
            .unwrap_or("Web_PKUHOLE_2.0.0_WEB_UUID_unknown")
            .to_string();

        let cookie_store = store.load_cookie_store()?;
        let client = client::build(cookie_store.clone())?;

        // 检查并完成短信验证（交互式）
        crate::verify::check_and_verify(&client, &session.token, &uuid).await?;

        // 验证后保存更新的 cookies
        store.save_cookie_store(&cookie_store)?;

        Ok(Self {
            client,
            token: session.token,
            uuid,
        })
    }

    // ─── 通用请求 ───────────────────────────────────────────

    async fn get<T: DeserializeOwned + Default>(&self, path: &str) -> Result<T> {
        let url = format!("{TREEHOLE_BASE}/chapi/api/v3{path}");
        let resp: ApiResp<T> = self
            .client
            .get(&url)
            .header("authorization", format!("Bearer {}", self.token))
            .header("uuid", &self.uuid)
            .send()
            .await
            .with_context(|| format!("GET {path} 失败"))?
            .json()
            .await
            .with_context(|| format!("解析 {path} 响应失败"))?;
        Self::check_resp(resp, path)
    }

    async fn post_json<T: DeserializeOwned + Default, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let url = format!("{TREEHOLE_BASE}/chapi/api/v3{path}");
        let resp: ApiResp<T> = self
            .client
            .post(&url)
            .header("authorization", format!("Bearer {}", self.token))
            .header("uuid", &self.uuid)
            .json(body)
            .send()
            .await
            .with_context(|| format!("POST {path} 失败"))?
            .json()
            .await
            .with_context(|| format!("解析 {path} 响应失败"))?;
        Self::check_resp(resp, path)
    }

    /// POST 请求，只关心是否成功，不关心 data 内容
    async fn post_action<B: Serialize>(&self, path: &str, body: &B) -> Result<String> {
        let url = format!("{TREEHOLE_BASE}/chapi/api/v3{path}");
        let resp: ApiResp<serde_json::Value> = self
            .client
            .post(&url)
            .header("authorization", format!("Bearer {}", self.token))
            .header("uuid", &self.uuid)
            .json(body)
            .send()
            .await
            .with_context(|| format!("POST {path} 失败"))?
            .json()
            .await
            .with_context(|| format!("解析 {path} 响应失败"))?;
        if resp.code == 40002 {
            return Err(anyhow!(
                "需要短信验证，请重新运行 `treehole login -p` 完成验证"
            ));
        }
        if resp.success {
            Ok(resp.message)
        } else {
            Err(anyhow!("{path}: {}", resp.message))
        }
    }

    fn check_resp<T>(resp: ApiResp<T>, path: &str) -> Result<T> {
        if resp.code == 40002 {
            return Err(anyhow!(
                "需要短信验证，请重新运行 `treehole login -p` 完成验证"
            ));
        }
        if resp.success {
            resp.data.ok_or_else(|| anyhow!("{path}: 成功但 data 为空"))
        } else {
            Err(anyhow!("{path}: {}", resp.message))
        }
    }

    /// GET 请求到 /chapi/api/ (非 v3) 路径
    async fn get_legacy<T: DeserializeOwned + Default>(&self, path: &str) -> Result<T> {
        let url = format!("{TREEHOLE_BASE}/chapi/api{path}");
        let resp: ApiResp<T> = self
            .client
            .get(&url)
            .header("authorization", format!("Bearer {}", self.token))
            .header("uuid", &self.uuid)
            .send()
            .await
            .with_context(|| format!("GET {path} 失败"))?
            .json()
            .await
            .with_context(|| format!("解析 {path} 响应失败"))?;
        Self::check_resp(resp, path)
    }

    // ─── Holes ──────────────────────────────────────────────

    /// 获取最新帖子列表（带评论预览）
    pub async fn list_holes(&self, page: u32, limit: u32) -> Result<Vec<HoleListItem>> {
        let data: serde_json::Value = self
            .get(&format!(
                "/hole/list_comments?page={page}&limit={limit}&comment_limit=3&comment_stream=1"
            ))
            .await?;
        parse_hole_list_items(&data)
    }

    /// 获取关注帖子
    pub async fn list_follow(&self, page: u32, limit: u32) -> Result<Vec<HoleListItem>> {
        let data: serde_json::Value = self
            .get(&format!(
                "/hole/list_comments?page={page}&limit={limit}&comment_limit=3&comment_stream=1&is_follow=1"
            ))
            .await?;
        parse_hole_list_items(&data)
    }

    /// 获取单个帖子详情（含全部评论）
    pub async fn get_hole(&self, pid: i64) -> Result<HoleWithComments> {
        let data: serde_json::Value = self
            .get(&format!("/hole/one?pid={pid}&comment_stream=1"))
            .await?;
        parse_hole_with_comments(&data)
    }

    /// 获取我的帖子
    pub async fn my_holes(&self, page: u32, limit: u32) -> Result<Vec<Hole>> {
        let data: ListData<Hole> = self
            .get(&format!("/hole/my_list?page={page}&limit={limit}"))
            .await?;
        Ok(data.list)
    }

    /// 搜索帖子
    pub async fn search(&self, keyword: &str, page: u32, limit: u32) -> Result<Vec<Hole>> {
        let encoded = urlencoding::encode(keyword);
        let data: ListData<Hole> = self
            .get(&format!(
                "/hole/list?keyword={encoded}&page={page}&limit={limit}"
            ))
            .await?;
        Ok(data.list)
    }

    // ─── 发帖/评论 ──────────────────────────────────────────

    /// 发布新帖子
    pub async fn create_hole(&self, req: &CreateHoleReq) -> Result<serde_json::Value> {
        self.post_json("/hole/post", req).await
    }

    /// 发表评论
    pub async fn create_comment(&self, req: &CreateCommentReq) -> Result<serde_json::Value> {
        self.post_json("/comment/post", req).await
    }

    // ─── 互动 ───────────────────────────────────────────────

    pub async fn praise_hole(&self, pid: i64) -> Result<String> {
        self.post_action("/hole/praise", &serde_json::json!({"pid": pid}))
            .await
    }

    pub async fn tread_hole(&self, pid: i64) -> Result<String> {
        self.post_action("/hole/tread", &serde_json::json!({"pid": pid}))
            .await
    }

    pub async fn follow_hole(&self, pid: i64) -> Result<String> {
        self.post_action(
            "/hole/attention",
            &serde_json::json!({"pid": pid, "switch": 1}),
        )
        .await
    }

    pub async fn unfollow_hole(&self, pid: i64) -> Result<String> {
        self.post_action("/hole/attention_cancel", &serde_json::json!({"pid": pid}))
            .await
    }

    /// 收藏帖子（关注 + 归入分组）
    pub async fn star_hole(&self, pid: i64, bookmark_id: Option<i64>) -> Result<String> {
        let mut body = serde_json::json!({"pid": pid, "switch": 1});
        if let Some(bid) = bookmark_id {
            body["bookmark_id"] = serde_json::json!(bid);
        }
        self.post_action("/hole/attention", &body).await
    }

    /// 获取收藏分组列表
    pub async fn list_bookmark_groups(&self) -> Result<Vec<Bookmark>> {
        let data: ListData<Bookmark> = self
            .get("/bookmark/list?page=1&limit=200")
            .await?;
        Ok(data.list)
    }

    pub async fn report_hole(&self, pid: i64, reason: &str) -> Result<String> {
        self.post_action(
            "/hole/report",
            &serde_json::json!({"pid": pid, "reason": reason}),
        )
        .await
    }

    // ─── 消息 ───────────────────────────────────────────────

    pub async fn list_messages(&self, page: u32, limit: u32) -> Result<Vec<Message>> {
        let data: ListData<Message> = self
            .get(&format!("/message/index?page={page}&limit={limit}"))
            .await?;
        Ok(data.list)
    }

    pub async fn unread_count(&self) -> Result<(i64, i64)> {
        let int: UnreadCount = self.get("/message/un_read?message_type=int_msg").await?;
        let sys: UnreadCount = self.get("/message/un_read?message_type=sys_msg").await?;
        Ok((int.count, sys.count))
    }

    pub async fn mark_read(&self, ids: &[i64]) -> Result<String> {
        self.post_action("/message/set_read", &serde_json::json!({"ids": ids}))
            .await
    }

    // ─── 用户 ───────────────────────────────────────────────

    pub async fn user_info(&self) -> Result<UserInfo> {
        let url = format!("{TREEHOLE_BASE}/chapi/api/v3/users/info");
        let resp: ApiResp<UserInfo> = self
            .client
            .post(&url)
            .header("authorization", format!("Bearer {}", self.token))
            .header("uuid", &self.uuid)
            .json(&serde_json::json!({}))
            .send()
            .await
            .context("获取用户信息失败")?
            .json()
            .await?;
        if resp.success {
            resp.data.ok_or_else(|| anyhow!("用户信息为空"))
        } else {
            Err(anyhow!("获取用户信息失败: {}", resp.message))
        }
    }

    // ─── 成绩 ───────────────────────────────────────────────

    pub async fn get_scores(&self) -> Result<ScoreData> {
        let data: serde_json::Value = self.get_legacy("/course/score_v2").await?;

        let score = data.get("score").ok_or_else(|| anyhow!("成绩数据缺少 score"))?;
        let gpa_section = data.get("gpa").ok_or_else(|| anyhow!("成绩数据缺少 gpa"))?;

        // Parse courses from score.cjxx
        let courses: Vec<CourseScore> = score.get("cjxx")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| serde_json::from_value(c.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();

        // Parse overall GPA from score.gpa
        let overall_gpa = score.get("gpa")
            .and_then(|g| g.get("gpa"))
            .and_then(|v| v.as_str())
            .unwrap_or("N/A")
            .to_string();
        let total_credits = score.get("gpa")
            .and_then(|g| g.get("xxxf"))
            .and_then(|v| v.as_str())
            .unwrap_or("0")
            .to_string();

        // Parse semester GPAs from gpa.data
        let semester_gpas: Vec<SemesterGpa> = gpa_section.get("data")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|g| serde_json::from_value(g.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();

        Ok(ScoreData {
            courses,
            semester_gpas,
            overall_gpa,
            total_credits,
        })
    }

    // ─── 课表 ───────────────────────────────────────────────

    pub async fn get_coursetable(&self) -> Result<Vec<CourseRow>> {
        let data: serde_json::Value = self.get_legacy("/getCoursetable_v2").await?;
        let courses = data.get("course")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("课表数据缺少 course"))?;

        let days = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
        let rows = courses.iter().map(|row| {
            let time_num = val_str(row, "timeNum");
            let mut slots: [Option<CourseSlot>; 7] = Default::default();
            for (i, day) in days.iter().enumerate() {
                if let Some(d) = row.get(*day) {
                    let name = val_str(d, "courseName");
                    if !name.is_empty() {
                        // Parse HTML: "CourseName<br>上课信息：...<br>考试信息：..."
                        let parts: Vec<&str> = name.split("<br>").collect();
                        let course_name = parts.first().unwrap_or(&"").to_string();
                        let style = val_str(d, "sty");
                        slots[i] = Some(CourseSlot {
                            course_name,
                            style,
                        });
                    }
                }
            }
            CourseRow { time_num, slots }
        }).collect();

        Ok(rows)
    }

    pub async fn get_class_times(&self) -> Result<Vec<ClassTime>> {
        let data: serde_json::Value = self.post_json("/classtimes/user_class_times", &serde_json::json!({})).await?;
        let list: Vec<ClassTime> = data.get("list")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| serde_json::from_value(c.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        Ok(list)
    }

    // ─── 学术日历 ───────────────────────────────────────────

    pub async fn list_lab_events(&self, start: &str, end: &str) -> Result<Vec<LabEvent>> {
        let data: serde_json::Value = self.get(&format!(
            "/lab_events?startDate={start}&endDate={end}"
        )).await?;
        let events: Vec<LabEvent> = data.get("events")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|e| serde_json::from_value(e.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        Ok(events)
    }

    // ─── 活动日历 ───────────────────────────────────────────

    pub async fn list_activity_events(&self, start: &str, end: &str, page: u32, limit: u32) -> Result<Vec<ActivityEvent>> {
        let data: ListData<ActivityEvent> = self.get(&format!(
            "/events/list?startTime={start}&endTime={end}&page={page}&limit={limit}"
        )).await?;
        Ok(data.list)
    }

    // ─── 周日程 ─────────────────────────────────────────────

    pub async fn list_schedules(&self, start: &str, end: &str) -> Result<Vec<ScheduleItem>> {
        let data: serde_json::Value = self.get(&format!(
            "/schedules/weeklySchedules?start_time={start}&end_time={end}"
        )).await?;
        // The data might be a direct array or have a list field
        if let Some(arr) = data.as_array() {
            Ok(arr.iter()
                .filter_map(|s| serde_json::from_value(s.clone()).ok())
                .collect())
        } else if let Some(list) = data.get("list").and_then(|v| v.as_array()) {
            Ok(list.iter()
                .filter_map(|s| serde_json::from_value(s.clone()).ok())
                .collect())
        } else {
            Ok(vec![])
        }
    }
}

// ─── 请求体 ─────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CreateHoleReq {
    pub text: String,
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags_ids: Option<String>,
    #[serde(default)]
    pub anonymous: i64,
    #[serde(default)]
    pub fold: i64,
    #[serde(default)]
    pub reward_cost: i64,
}

#[derive(Debug, Serialize)]
pub struct CreateCommentReq {
    pub pid: i64,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_id: Option<i64>,
    #[serde(default)]
    pub anonymous: i64,
}

// ─── 手动解析（避免 serde 在数组/对象混合字段上报错）─────────────

fn val_i64(v: &serde_json::Value, key: &str) -> i64 {
    v.get(key).and_then(|x| x.as_i64()).unwrap_or(0)
}
fn val_str(v: &serde_json::Value, key: &str) -> String {
    v.get(key).and_then(|x| x.as_str()).map(|s| s.to_string()).unwrap_or_default()
}

fn parse_tags(v: &serde_json::Value) -> Vec<TagInfo> {
    v.get("tags_info")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .map(|t| TagInfo {
                    tag_name: val_str(t, "tag_name"),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_comment(c: &serde_json::Value) -> Comment {
    Comment {
        cid: val_i64(c, "cid"),
        text: val_str(c, "text"),
        timestamp: val_i64(c, "timestamp"),
        name_tag: val_str(c, "name_tag"),
        is_lz: val_i64(c, "is_lz"),
        quote: c.get("quote").cloned().unwrap_or(serde_json::Value::Null),
    }
}

fn parse_comments(v: &serde_json::Value, key: &str) -> Vec<Comment> {
    v.get(key)
        .and_then(|c| c.as_array())
        .map(|arr| arr.iter().map(parse_comment).collect())
        .unwrap_or_default()
}

fn parse_hole(v: &serde_json::Value) -> Hole {
    Hole {
        pid: val_i64(v, "pid"),
        text: val_str(v, "text"),
        timestamp: val_i64(v, "timestamp"),
        reply: val_i64(v, "reply"),
        likenum: val_i64(v, "likenum"),
        tread_num: val_i64(v, "tread_num"),
        is_follow: val_i64(v, "is_follow"),
        is_top: val_i64(v, "is_top"),
        reward_cost: val_i64(v, "reward_cost"),
        tags_info: parse_tags(v),
    }
}

fn parse_hole_list_items(data: &serde_json::Value) -> Result<Vec<HoleListItem>> {
    let list = data
        .get("list")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("响应缺少 list 数组"))?;

    let items = list
        .iter()
        .map(|v| HoleListItem {
            pid: val_i64(v, "pid"),
            text: val_str(v, "text"),
            timestamp: val_i64(v, "timestamp"),
            reply: val_i64(v, "reply"),
            likenum: val_i64(v, "likenum"),
            tread_num: val_i64(v, "tread_num"),
            is_follow: val_i64(v, "is_follow"),
            is_top: val_i64(v, "is_top"),
            reward_cost: val_i64(v, "reward_cost"),
            tags_info: parse_tags(v),
            comment_list: parse_comments(v, "comment_list"),
        })
        .collect();
    Ok(items)
}

fn parse_hole_with_comments(data: &serde_json::Value) -> Result<HoleWithComments> {
    let hole_val = data
        .get("hole")
        .ok_or_else(|| anyhow::anyhow!("响应缺少 hole"))?;
    Ok(HoleWithComments {
        hole: parse_hole(hole_val),
        list: parse_comments(data, "list"),
        total: data.get("total").and_then(|x| x.as_i64()),
    })
}
