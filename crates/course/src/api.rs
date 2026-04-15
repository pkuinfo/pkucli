//! 教学网 API 客户端
//!
//! 基于 Blackboard Learn 平台，通过 HTML 解析获取课程信息。
//! 参考 pku3b 项目实现。

use anyhow::{anyhow, Context, Result};
use chrono::TimeZone;
use scraper::{Html, Selector};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::client::{self, COURSE_BASE};
use crate::multipart::MultipartBuilder;
use pkuinfo_common::session::Store;

const APP_NAME: &str = "course";

// ─── 数据模型 ──────────────────────────────────────────────────

/// 课程基础信息
#[derive(Debug, Clone)]
pub struct CourseInfo {
    /// 课程内部 ID（如 _80052_1）
    pub id: String,
    /// 课程完整标题（含学期信息）
    pub long_title: String,
    /// 是否为当前学期
    pub is_current: bool,
}

impl CourseInfo {
    /// 课程名（去掉前缀 ID）
    pub fn title(&self) -> &str {
        self.long_title
            .split_once(':')
            .map(|(_, t)| t.trim())
            .unwrap_or(&self.long_title)
    }

    /// 纯课程名（去掉学期后缀）
    pub fn name(&self) -> &str {
        let t = self.title();
        // 找最后一个 '(' 作为学期信息起点
        t.rfind('(')
            .map(|i| t[..i].trim())
            .unwrap_or(t)
    }
}

/// 课程侧边栏入口
#[derive(Debug, Clone)]
pub struct CourseEntry {
    pub name: String,
    pub url: String,
}

/// 课程内容项（支持递归发现）
#[derive(Debug, Clone)]
pub struct ContentItem {
    /// 内容 ID（从 HTML 元素中提取）
    pub id: String,
    /// 内容标题
    pub title: String,
    /// 内容类型描述
    pub item_type: ContentType,
    /// 详情链接
    pub url: Option<String>,
    /// 附件列表
    pub attachments: Vec<Attachment>,
    /// 描述/说明
    pub description: String,
    /// 是否含有可点击的链接（用于递归发现）
    pub has_link: bool,
}

/// 内容类型
#[derive(Debug, Clone, PartialEq)]
pub enum ContentType {
    Assignment,
    Document,
    Folder,
}

/// 附件信息
#[derive(Debug, Clone)]
pub struct Attachment {
    pub name: String,
    pub url: String,
}

/// 作业详情
#[derive(Debug, Clone)]
pub struct AssignmentDetail {
    /// 作业标题
    pub title: String,
    /// 截止时间（原始字符串）
    pub deadline: Option<String>,
    /// 说明文本
    pub instructions: String,
    /// 附件
    pub attachments: Vec<Attachment>,
    /// 提交状态
    pub status: String,
}

/// 跨课程作业汇总（用于统一列表）
#[derive(Debug, Clone)]
pub struct AssignmentSummary {
    /// 哈希 ID（course_id + content_id 哈希）
    pub hash_id: String,
    /// 所属课程名称
    pub course_name: String,
    /// 所属课程 ID
    pub course_id: String,
    /// 内容 ID
    pub content_id: String,
    /// 作业标题
    pub title: String,
    /// 截止时间（原始字符串）
    pub deadline_raw: Option<String>,
    /// 截止时间（解析后）
    pub deadline: Option<chrono::DateTime<chrono::Local>>,
    /// 附件列表
    pub attachments: Vec<Attachment>,
    /// 说明
    pub descriptions: Vec<String>,
    /// 最近提交尝试
    pub last_attempt: Option<String>,
}

/// 课程回放信息
#[derive(Debug, Clone)]
pub struct VideoInfo {
    /// 视频标题
    pub title: String,
    /// 录制时间
    pub time: String,
    /// 视频页面 URL（绝对路径）
    pub url: String,
    /// 所属课程名称
    pub course_name: String,
    /// 哈希 ID
    pub hash_id: String,
}

/// 课程公告
#[derive(Debug, Clone)]
pub struct Announcement {
    /// 公告标题
    pub title: String,
    /// 公告正文
    pub body: String,
    /// 发布日期（原始字符串）
    pub date: String,
    /// 发布者
    pub author: String,
}

/// 跨课程公告汇总（含所属课程名）
#[derive(Debug, Clone)]
pub struct AnnouncementSummary {
    /// 所属课程名称
    pub course_name: String,
    /// 公告详情
    pub announcement: Announcement,
}

/// 视频下载所需的详细信息
#[derive(Debug)]
pub struct VideoDetail {
    /// m3u8 基础 URL（用于拼接 segment 相对路径）
    pub base_url: url::Url,
    /// 解析后的媒体播放列表
    pub playlist: m3u8_rs::MediaPlaylist,
}

// ─── 哈希 ID 工具 ─────────────────────────────────────────────

/// 从多个字符串计算哈希 ID（与 pku3b 兼容的格式）
pub fn compute_hash_id(parts: &[&str]) -> String {
    let mut hasher = std::hash::DefaultHasher::new();
    for p in parts {
        p.hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}

// ─── 截止时间解析 ─────────────────────────────────────────────

/// 解析 Blackboard 中文格式的截止时间
/// 格式: "2025年3月15日 星期六 下午11:59"
pub fn parse_deadline(raw: &str) -> Option<chrono::DateTime<chrono::Local>> {
    let re = regex::Regex::new(
        r"(\d{4})年(\d{1,2})月(\d{1,2})日 星期. (上午|下午)(\d{1,2}):(\d{1,2})",
    )
    .ok()?;

    let caps = re.captures(raw)?;
    let year: i32 = caps[1].parse().ok()?;
    let month: u32 = caps[2].parse().ok()?;
    let day: u32 = caps[3].parse().ok()?;
    let mut hour: u32 = caps[5].parse().ok()?;
    let minute: u32 = caps[6].parse().ok()?;

    if &caps[4] == "下午" && hour < 12 {
        hour += 12;
    }
    if &caps[4] == "上午" && hour == 12 {
        hour = 0;
    }

    let naive_dt = chrono::NaiveDateTime::new(
        chrono::NaiveDate::from_ymd_opt(year, month, day)?,
        chrono::NaiveTime::from_hms_opt(hour, minute, 0)?,
    );

    chrono::Local.from_local_datetime(&naive_dt).single()
}

/// 格式化截止时间倒计时
pub fn fmt_time_delta(delta: chrono::TimeDelta) -> String {
    use colored::Colorize;

    if delta < chrono::TimeDelta::zero() {
        return "已截止".red().to_string();
    }

    let mut secs = delta.num_seconds() as u64;
    let mut parts = Vec::new();

    if secs >= 86400 {
        parts.push(format!("{}d", secs / 86400));
        secs %= 86400;
    }
    if secs >= 3600 {
        parts.push(format!("{}h", secs / 3600));
        secs %= 3600;
    }
    if secs >= 60 {
        parts.push(format!("{}m", secs / 60));
        secs %= 60;
    }
    parts.push(format!("{}s", secs));

    let text = format!("剩余 {}", parts.join(" "));
    if delta > chrono::TimeDelta::days(1) {
        text.yellow().to_string()
    } else {
        text.red().to_string()
    }
}

// ─── HTML 解析工具 ────────────────────────────────────────────

/// 从 HTML DOM 中解析内容项列表
fn parse_content_items(dom: &Html) -> Result<Vec<ContentItem>> {
    let item_sel = Selector::parse("#content_listContainer > li, li.clearfix").unwrap();
    let title_sel = Selector::parse("h3").unwrap();
    let link_sel = Selector::parse("h3 a").unwrap();
    let details_sel = Selector::parse("div.details").unwrap();
    let desc_sel = Selector::parse("div.vtbegenerated").unwrap();
    let attach_sel = Selector::parse("ul.attachments li a").unwrap();
    let img_sel = Selector::parse("img").unwrap();

    let mut items = Vec::new();

    for li in dom.select(&item_sel) {
        // 尝试从 element id 获取 content ID
        let id = li
            .value()
            .attr("id")
            .map(|s| s.to_string())
            .or_else(|| {
                // 有些 li 没有 id，从内部链接的 content_id 参数获取
                li.select(&link_sel)
                    .next()
                    .and_then(|a| a.value().attr("href"))
                    .and_then(|href| {
                        let url = reqwest::Url::parse(href)
                            .or_else(|_| reqwest::Url::parse(&format!("{COURSE_BASE}{href}")))
                            .ok()?;
                        url.query_pairs()
                            .find(|(k, _)| k == "content_id")
                            .map(|(_, v)| v.to_string())
                    })
            })
            .unwrap_or_default();

        let title = li
            .select(&title_sel)
            .next()
            .map(|h| h.text().collect::<String>())
            .unwrap_or_default()
            .trim()
            .to_string();

        if title.is_empty() {
            continue;
        }

        let has_link = li.select(&link_sel).next().is_some();

        let url = li
            .select(&link_sel)
            .next()
            .and_then(|a| a.value().attr("href"))
            .map(|h| {
                if h.starts_with("http") {
                    h.to_string()
                } else {
                    format!("{COURSE_BASE}{h}")
                }
            });

        // 优先从 vtbegenerated 获取描述（与 pku3b 一致），fallback 到 details
        let description = li
            .select(&desc_sel)
            .next()
            .or_else(|| li.select(&details_sel).next())
            .map(|d| d.text().collect::<String>())
            .unwrap_or_default()
            .trim()
            .to_string();

        let attachments: Vec<Attachment> = li
            .select(&attach_sel)
            .filter_map(|a| {
                let name = a.text().collect::<String>().trim().to_string();
                // 去掉 nbsp 前缀
                let name = name.strip_prefix('\u{a0}').unwrap_or(&name).to_string();
                let href = a.value().attr("href")?;
                let full_url = if href.starts_with("http") {
                    href.to_string()
                } else {
                    format!("{COURSE_BASE}{href}")
                };
                Some(Attachment {
                    name,
                    url: full_url,
                })
            })
            .collect();

        // 判断内容类型（参考 pku3b 的 img alt 属性判断）
        let img_alt = li
            .select(&img_sel)
            .next()
            .and_then(|img| img.value().attr("alt"))
            .unwrap_or_default();

        let item_type = if img_alt == "作业"
            || url
                .as_deref()
                .is_some_and(|u| u.contains("uploadAssignment") || u.contains("assignment"))
        {
            ContentType::Assignment
        } else if url.as_deref().is_some_and(|u| u.contains("listContent")) {
            ContentType::Folder
        } else {
            ContentType::Document
        };

        items.push(ContentItem {
            id,
            title,
            item_type,
            url,
            attachments,
            description,
            has_link,
        });
    }

    Ok(items)
}

// ─── API 客户端 ────────────────────────────────────────────────

pub struct CourseApi {
    client: reqwest::Client,
}

impl CourseApi {
    /// 从已保存的会话创建 API 客户端
    pub fn from_session() -> Result<Self> {
        let store = Store::new(APP_NAME)?;
        let session = store
            .load_session()?
            .ok_or_else(|| anyhow!("未登录，请先运行 `course login`"))?;

        if session.is_expired() {
            return Err(anyhow!("会话已过期，请重新运行 `course login`"));
        }

        let cookie_store = store.load_cookie_store()?;
        let client = client::build(cookie_store)?;
        Ok(Self { client })
    }

    /// 获取教学网主页 HTML
    async fn get_homepage(&self) -> Result<Html> {
        let url = format!(
            "{COURSE_BASE}/webapps/portal/execute/tabs/tabAction?tab_tab_group_id=_1_1"
        );
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("获取教学网主页失败")?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(anyhow!("会话已失效，请重新运行 `course login`"));
        }
        if !status.is_success() {
            return Err(anyhow!("教学网主页返回 HTTP {}", status));
        }

        let final_url = resp.url().clone();
        let body = resp.text().await?;

        if final_url.path().contains("login")
            || final_url.host_str().is_some_and(|h| h.contains("iaaa"))
            || body.contains("IAAA") && body.contains("login")
        {
            return Err(anyhow!("会话已失效（被重定向到登录页），请重新运行 `course login`"));
        }

        Ok(Html::parse_document(&body))
    }

    /// 获取课程列表
    pub async fn list_courses(&self, only_current: bool) -> Result<Vec<CourseInfo>> {
        let dom = self.get_homepage().await?;

        let re = regex::Regex::new(r"key=([\d_]+),").unwrap();
        let portlet_sel = Selector::parse("div.portlet").unwrap();
        let title_sel = Selector::parse("span.moduleTitle").unwrap();
        let ul_sel = Selector::parse("ul.courseListing").unwrap();
        let li_a_sel = Selector::parse("li a").unwrap();

        let mut courses = Vec::new();

        for portlet in dom.select(&portlet_sel) {
            let title_text = portlet
                .select(&title_sel)
                .next()
                .map(|el| el.text().collect::<String>())
                .unwrap_or_default();

            let is_current =
                title_text.contains("当前") || title_text.contains("Current Semester");

            for ul in portlet.select(&ul_sel) {
                for a in ul.select(&li_a_sel) {
                    let href = a.value().attr("href").unwrap_or_default();
                    let text = a.text().collect::<String>();

                    if let Some(caps) = re.captures(href) {
                        if let Some(key) = caps.get(1) {
                            courses.push(CourseInfo {
                                id: key.as_str().to_string(),
                                long_title: text.trim().to_string(),
                                is_current,
                            });
                        }
                    }
                }
            }
        }

        if only_current {
            courses.retain(|c| c.is_current);
        }

        Ok(courses)
    }

    /// 获取课程主页（公告页面）
    async fn get_course_page(&self, course_id: &str) -> Result<Html> {
        let url = format!(
            "{COURSE_BASE}/webapps/blackboard/execute/announcement\
             ?method=search&context=course_entry&course_id={course_id}\
             &handle=announcements_entry&mode=view"
        );
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!("获取课程页面失败: HTTP {}", resp.status()));
        }
        let body = resp.text().await?;
        Ok(Html::parse_document(&body))
    }

    /// 获取课程侧边栏导航入口
    pub async fn list_course_entries(&self, course_id: &str) -> Result<Vec<CourseEntry>> {
        let dom = self.get_course_page(course_id).await?;
        let sel = Selector::parse("#courseMenuPalette_contents > li > a").unwrap();

        let entries: Vec<CourseEntry> = dom
            .select(&sel)
            .filter_map(|a| {
                let text = a.text().collect::<String>();
                let href = a.value().attr("href")?;
                Some(CourseEntry {
                    name: text.trim().to_string(),
                    url: href.to_string(),
                })
            })
            .collect();

        Ok(entries)
    }

    /// 获取内容列表页面（作业、文件等）
    pub async fn list_content(
        &self,
        course_id: &str,
        content_id: &str,
    ) -> Result<Vec<ContentItem>> {
        let url = format!(
            "{COURSE_BASE}/webapps/blackboard/content/listContent.jsp\
             ?content_id={content_id}&course_id={course_id}"
        );
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!("获取内容列表失败: HTTP {}", resp.status()));
        }
        let body = resp.text().await?;
        let dom = Html::parse_document(&body);

        parse_content_items(&dom)
    }

    /// 递归获取课程下的所有内容项（类似 pku3b 的 CourseContentStream）
    ///
    /// 从侧边栏入口出发，递归进入所有 Folder 类型页面，
    /// 收集所有 Assignment / Document / Folder 内容。
    pub async fn list_all_content_recursive(
        &self,
        course_id: &str,
    ) -> Result<Vec<ContentItem>> {
        // 获取侧边栏入口
        let entries = self.list_course_entries(course_id).await?;

        // 提取所有 listContent 页面的 content_id 作为初始探测点
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: Vec<String> = Vec::new();

        for entry in &entries {
            if let Some((cid, _)) = Self::parse_content_url(&entry.url) {
                if visited.insert(cid.clone()) {
                    queue.push(cid);
                }
            }
        }

        let mut all_items = Vec::new();

        // BFS 逐层探索
        while !queue.is_empty() {
            // 批量取出（最多 8 个并发）
            let batch: Vec<String> = queue
                .drain(..queue.len().min(8))
                .collect();

            let futures: Vec<_> = batch
                .iter()
                .map(|cid| self.list_content(course_id, cid))
                .collect();

            let results = futures::future::join_all(futures).await;

            for result in results {
                let items = match result {
                    Ok(items) => items,
                    Err(e) => {
                        tracing::warn!("获取内容列表失败: {e:#}");
                        continue;
                    }
                };

                for item in items {
                    // 如果是文件夹且有链接，加入探测队列
                    if item.item_type == ContentType::Folder && item.has_link {
                        if let Some(url) = &item.url {
                            if let Some((cid, _)) = Self::parse_content_url(url) {
                                if visited.insert(cid.clone()) {
                                    queue.push(cid);
                                }
                            }
                        }
                    }

                    all_items.push(item);
                }
            }
        }

        Ok(all_items)
    }

    /// 获取课程下所有作业的汇总信息（递归发现 + 详情获取）
    pub async fn list_assignments_for_course(
        &self,
        course: &CourseInfo,
    ) -> Result<Vec<AssignmentSummary>> {
        let all_content = self.list_all_content_recursive(&course.id).await?;

        let assignments: Vec<_> = all_content
            .into_iter()
            .filter(|item| item.item_type == ContentType::Assignment)
            .collect();

        let mut summaries = Vec::new();
        for item in &assignments {
            let hash_id = compute_hash_id(&[&course.id, &item.id]);

            // 尝试获取作业详情
            let (deadline_raw, deadline, attempt) =
                match self.get_assignment(&course.id, &item.id).await {
                    Ok(detail) => {
                        let dl = detail.deadline.as_deref().and_then(parse_deadline);
                        (detail.deadline, dl, None)
                    }
                    Err(_) => (None, None, None),
                };

            // 尝试获取提交记录
            let attempt = match attempt {
                Some(a) => Some(a),
                None => self
                    .get_assignment_attempt(&course.id, &item.id)
                    .await
                    .unwrap_or(None),
            };

            summaries.push(AssignmentSummary {
                hash_id,
                course_name: course.name().to_string(),
                course_id: course.id.clone(),
                content_id: item.id.clone(),
                title: item.title.clone(),
                deadline_raw,
                deadline,
                attachments: item.attachments.clone(),
                descriptions: if item.description.is_empty() {
                    vec![]
                } else {
                    vec![item.description.clone()]
                },
                last_attempt: attempt,
            });
        }

        Ok(summaries)
    }

    /// 获取作业详情
    pub async fn get_assignment(
        &self,
        course_id: &str,
        content_id: &str,
    ) -> Result<AssignmentDetail> {
        let url = format!(
            "{COURSE_BASE}/webapps/assignment/uploadAssignment\
             ?action=newAttempt&content_id={content_id}&course_id={course_id}"
        );
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!("获取作业详情失败: HTTP {}", resp.status()));
        }
        let body = resp.text().await?;
        let dom = Html::parse_document(&body);

        let title = dom
            .select(&Selector::parse("span.title").unwrap())
            .next()
            .or_else(|| dom.select(&Selector::parse("#pageTitleText").unwrap()).next())
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default()
            .trim()
            .to_string();

        // 截止时间
        let deadline = dom
            .select(&Selector::parse(".itemdates").unwrap())
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string());

        // 说明
        let instructions = dom
            .select(&Selector::parse("div.vtbegenerated").unwrap())
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        // 附件
        let attach_sel = Selector::parse("ul.attachments li a").unwrap();
        let attachments: Vec<Attachment> = dom
            .select(&attach_sel)
            .filter_map(|a| {
                let name = a.text().collect::<String>().trim().to_string();
                let href = a.value().attr("href")?;
                let full_url = if href.starts_with("http") {
                    href.to_string()
                } else {
                    format!("{COURSE_BASE}{href}")
                };
                Some(Attachment { name, url: full_url })
            })
            .collect();

        // 提交状态
        let status = dom
            .select(&Selector::parse(".status").unwrap())
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| "未知".to_string());

        Ok(AssignmentDetail {
            title,
            deadline,
            instructions,
            attachments,
            status,
        })
    }

    /// 下载文件
    pub async fn download_file(&self, url: &str) -> Result<(String, Vec<u8>)> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .context("下载文件失败")?;

        if !resp.status().is_success() {
            return Err(anyhow!("下载失败: HTTP {}", resp.status()));
        }

        // 从 content-disposition 或 URL 提取文件名
        let filename = resp
            .headers()
            .get("content-disposition")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| {
                v.split("filename=").nth(1).map(|s| {
                    s.trim_matches('"').to_string()
                })
            })
            .unwrap_or_else(|| {
                url.rsplit('/')
                    .next()
                    .unwrap_or("download")
                    .split('?')
                    .next()
                    .unwrap_or("download")
                    .to_string()
            });

        let bytes = resp.bytes().await?.to_vec();
        Ok((filename, bytes))
    }

    /// 从课程侧边栏入口 URL 提取 content_id 和 course_id
    pub fn parse_content_url(url: &str) -> Option<(String, String)> {
        let parsed = reqwest::Url::parse(url)
            .or_else(|_| reqwest::Url::parse(&format!("{COURSE_BASE}{url}")))
            .ok()?;

        let mut content_id = None;
        let mut course_id = None;

        for (k, v) in parsed.query_pairs() {
            match k.as_ref() {
                "content_id" => content_id = Some(v.to_string()),
                "course_id" => course_id = Some(v.to_string()),
                _ => {}
            }
        }

        Some((content_id?, course_id?))
    }

    // ─── 作业提交相关 ─────────────────────────────────────────────

    /// 获取作业历史提交页面（查看最近一次提交尝试）
    pub async fn get_assignment_attempt(
        &self,
        course_id: &str,
        content_id: &str,
    ) -> Result<Option<String>> {
        let url = format!(
            "{COURSE_BASE}/webapps/assignment/uploadAssignment\
             ?mode=view&content_id={content_id}&course_id={course_id}"
        );
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Ok(None);
        }
        let body = resp.text().await?;
        let dom = Html::parse_document(&body);

        let sel = Selector::parse("h3#currentAttempt_label").unwrap();
        Ok(dom.select(&sel).next().map(|el| {
            el.text()
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        }))
    }

    /// 获取作业提交表单的隐藏字段
    pub async fn get_submit_formfields(
        &self,
        course_id: &str,
        content_id: &str,
    ) -> Result<HashMap<String, String>> {
        let url = format!(
            "{COURSE_BASE}/webapps/assignment/uploadAssignment\
             ?action=newAttempt&content_id={content_id}&course_id={course_id}"
        );
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!("获取提交表单失败: HTTP {}", resp.status()));
        }
        let body = resp.text().await?;
        let dom = Html::parse_document(&body);

        let extract_field = |input: scraper::ElementRef<'_>| {
            let name = input.value().attr("name")?.to_owned();
            let value = input.value().attr("value")?.to_owned();
            Some((name, value))
        };

        let fields = dom
            .select(&Selector::parse("form#uploadAssignmentFormId input").unwrap())
            .filter_map(extract_field)
            .chain(
                dom.select(&Selector::parse("div.field input").unwrap())
                    .filter_map(extract_field),
            )
            .collect::<HashMap<_, _>>();

        Ok(fields)
    }

    /// 提交作业文件
    pub async fn submit_assignment(
        &self,
        course_id: &str,
        content_id: &str,
        file_path: &Path,
    ) -> Result<()> {
        let ext = file_path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let content_type = get_mime_type(&ext);

        let filename = file_path
            .file_name()
            .context("文件名获取失败")?
            .to_string_lossy()
            .to_string();

        let map = self.get_submit_formfields(course_id, content_id).await?;

        macro_rules! field_val {
            ($name:expr) => {
                map.get($name)
                    .with_context(|| format!("表单字段 '{}' 未找到", $name))?
                    .as_bytes()
            };
        }

        let body = MultipartBuilder::new()
            .add_field("attempt_id", field_val!("attempt_id"))
            .add_field(
                "blackboard.platform.security.NonceUtil.nonce",
                field_val!("blackboard.platform.security.NonceUtil.nonce"),
            )
            .add_field(
                "blackboard.platform.security.NonceUtil.nonce.ajax",
                field_val!("blackboard.platform.security.NonceUtil.nonce.ajax"),
            )
            .add_field("content_id", field_val!("content_id"))
            .add_field("course_id", field_val!("course_id"))
            .add_field("isAjaxSubmit", field_val!("isAjaxSubmit"))
            .add_field("lu_link_id", field_val!("lu_link_id"))
            .add_field("mode", field_val!("mode"))
            .add_field("recallUrl", field_val!("recallUrl"))
            .add_field("remove_file_id", field_val!("remove_file_id"))
            .add_field(
                "studentSubmission.text_f",
                field_val!("studentSubmission.text_f"),
            )
            .add_field(
                "studentSubmission.text_w",
                field_val!("studentSubmission.text_w"),
            )
            .add_field(
                "studentSubmission.type",
                field_val!("studentSubmission.type"),
            )
            .add_field(
                "student_commentstext_f",
                field_val!("student_commentstext_f"),
            )
            .add_field(
                "student_commentstext_w",
                field_val!("student_commentstext_w"),
            )
            .add_field("student_commentstype", field_val!("student_commentstype"))
            .add_field("textbox_prefix", field_val!("textbox_prefix"))
            .add_field("studentSubmission.text", b"")
            .add_field("student_commentstext", b"")
            .add_field("dispatch", b"submit")
            .add_field("newFile_artifactFileId", b"undefined")
            .add_field("newFile_artifactType", b"undefined")
            .add_field("newFile_artifactTypeResourceKey", b"undefined")
            .add_field("newFile_attachmentType", b"L")
            .add_field("newFile_fileId", b"new")
            .add_field("newFile_linkTitle", filename.as_bytes())
            .add_field("newFilefilePickerLastInput", b"dummyValue")
            .add_file(
                "newFile_LocalFile0",
                &filename,
                content_type,
                std::fs::File::open(file_path).context("打开文件失败")?,
            )
            .add_field("useless", b"");

        let boundary = body.boundary().to_owned();
        let body_bytes = body.build().context("构建 multipart 表单失败")?;

        let upload_url = format!(
            "{COURSE_BASE}/webapps/assignment/uploadAssignment?action=submit"
        );
        let resp = self
            .client
            .post(&upload_url)
            .header("origin", COURSE_BASE)
            .header("accept", "*/*")
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(body_bytes)
            .send()
            .await
            .context("提交作业请求失败")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            if text.contains("尝试呈现错误页面时发生严重的内部错误") {
                return Err(anyhow!("提交失败: HTTP {} (服务器内部错误)", status));
            }
            return Err(anyhow!("提交失败: HTTP {}", status));
        }

        Ok(())
    }

    // ─── 公告相关 ──────────────────────────────────────────────────

    /// 获取指定课程的公告列表
    pub async fn list_announcements(&self, course_id: &str) -> Result<Vec<Announcement>> {
        let dom = self.get_course_page(course_id).await?;

        // 真实 Blackboard Learn 公告 DOM（通过 Chrome DevTools 验证）:
        //   <ul id="announcementList">
        //     <li class="clearfix" id="_93301_1">
        //       <h3 class="item">标题</h3>
        //       <div class="details">
        //         <p><span>发布时间: 2025年8月27日 ...</span></p>
        //         <p><div class="vtbegenerated">正文</div></p>
        //       </div>
        //       <div class="announcementInfo">
        //         <p><span>发帖者:</span> 作者</p>
        //         <p><span>发布至:</span> 课程名</p>
        //       </div>
        //     </li>
        //   </ul>

        let list_sel = Selector::parse("#announcementList > li").unwrap();
        let title_sel = Selector::parse("h3").unwrap();
        let body_sel = Selector::parse(".vtbegenerated").unwrap();
        let details_sel = Selector::parse(".details").unwrap();
        let info_sel = Selector::parse(".announcementInfo").unwrap();

        let mut announcements = Vec::new();

        for li in dom.select(&list_sel) {
            let title = li
                .select(&title_sel)
                .next()
                .map(|el| el.text().collect::<String>())
                .unwrap_or_default()
                .trim()
                .to_string();

            if title.is_empty() {
                continue;
            }

            let body = li
                .select(&body_sel)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            // 日期：.details 内第一个 <p> 的文本，格式 "发布时间: ..."
            let date = li
                .select(&details_sel)
                .next()
                .and_then(|el| {
                    let text = el.text().collect::<String>();
                    text.split("发布时间:")
                        .nth(1)
                        .map(|s| s.lines().next().unwrap_or("").trim().to_string())
                })
                .unwrap_or_default();

            // 作者：.announcementInfo 内 "发帖者:" 后的文本
            let author = li
                .select(&info_sel)
                .next()
                .and_then(|el| {
                    let text = el.text().collect::<String>();
                    text.split("发帖者:")
                        .nth(1)
                        .and_then(|s| s.lines().next())
                        .map(|s| s.trim().to_string())
                })
                .unwrap_or_default();

            announcements.push(Announcement {
                title,
                body,
                date,
                author,
            });
        }

        Ok(announcements)
    }

    /// 获取某门课程的公告列表，并附带课程名称
    pub async fn list_announcements_for_course(
        &self,
        course: &CourseInfo,
    ) -> Result<Vec<AnnouncementSummary>> {
        let announcements = self.list_announcements(&course.id).await?;
        Ok(announcements
            .into_iter()
            .map(|a| AnnouncementSummary {
                course_name: course.name().to_string(),
                announcement: a,
            })
            .collect())
    }

    // ─── 课程回放相关 ─────────────────────────────────────────────

    /// 获取课程回放列表
    pub async fn list_videos(
        &self,
        course_id: &str,
        course_name: &str,
    ) -> Result<Vec<VideoInfo>> {
        let url = format!(
            "{COURSE_BASE}/webapps/bb-streammedia-hqy-BBLEARN/videoList.action\
             ?sortDir=ASCENDING&numResults=100&editPaging=false\
             &course_id={course_id}&mode=view&startIndex=0"
        );
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!("获取回放列表失败: HTTP {}", resp.status()));
        }
        let body = resp.text().await?;
        let dom = Html::parse_document(&body);

        let base_url =
            url::Url::parse(&format!("{COURSE_BASE}/webapps/bb-streammedia-hqy-BBLEARN/"))
                .unwrap();

        let row_sel = Selector::parse("tbody#listContainer_databody > tr").unwrap();
        let span_sel = Selector::parse("span.table-data-cell-value").unwrap();

        let mut videos = Vec::new();
        for tr in dom.select(&row_sel) {
            let title = tr
                .child_elements()
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let mut spans = tr.select(&span_sel);
            let time = spans
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            // 跳过教师列
            let _ = spans.next();

            let link = spans
                .next()
                .and_then(|el| el.child_elements().next())
                .and_then(|a| a.value().attr("href"))
                .unwrap_or_default();

            let full_url = base_url
                .join(link)
                .map(|u| u.to_string())
                .unwrap_or_default();

            if !title.is_empty() {
                let hash_id = compute_hash_id(&[course_id, &title, &time]);
                videos.push(VideoInfo {
                    title,
                    time,
                    url: full_url,
                    course_name: course_name.to_string(),
                    hash_id,
                });
            }
        }

        Ok(videos)
    }

    /// 获取视频 iframe 跳转后的 URL（包含 course_id, sub_id, app_id, auth_data 参数）
    async fn get_video_redirect_url(&self, video_page_url: &str) -> Result<String> {
        let resp = self
            .client
            .get(video_page_url)
            .send()
            .await
            .context("获取视频页面失败")?;
        if !resp.status().is_success() {
            return Err(anyhow!("视频页面 HTTP {}", resp.status()));
        }
        let body = resp.text().await?;
        let dom = Html::parse_document(&body);

        let iframe = dom
            .select(&Selector::parse("#content iframe").unwrap())
            .next()
            .context("视频 iframe 未找到")?;
        let src = iframe.value().attr("src").context("iframe src 未找到")?;

        // 将相对路径补全为绝对路径
        let full_src = if src.starts_with("http") {
            src.to_string()
        } else {
            format!("{COURSE_BASE}{src}")
        };

        // 这个请求会 302 跳转到包含 sub_id 等参数的 URL
        let resp = self
            .client
            .get(&full_src)
            .send()
            .await
            .context("获取视频 iframe 内容失败")?;

        // reqwest 默认跟随重定向，最终 URL 就是我们需要的
        Ok(resp.url().to_string())
    }

    /// 获取视频的 m3u8 播放列表 URL
    async fn get_video_m3u8_url(&self, redirect_url: &str) -> Result<String> {
        let parsed = url::Url::parse(redirect_url).context("解析视频跳转 URL 失败")?;
        let params: HashMap<_, _> = parsed.query_pairs().collect();

        let course_id = params.get("course_id").context("缺少 course_id 参数")?;
        let sub_id = params.get("sub_id").context("缺少 sub_id 参数")?;
        let app_id = params.get("app_id").context("缺少 app_id 参数")?;
        let auth_data = params.get("auth_data").context("缺少 auth_data 参数")?;

        let sub_info_url = format!(
            "https://yjapise.pku.edu.cn/courseapi/v2/schedule/get-sub-info-by-auth-data\
             ?all=1&course_id={course_id}&sub_id={sub_id}\
             &with_sub_data=1&app_id={app_id}&auth_data={auth_data}"
        );

        let resp = self
            .client
            .get(&sub_info_url)
            .send()
            .await
            .context("获取视频元数据失败")?;

        if !resp.status().is_success() {
            return Err(anyhow!("视频元数据 HTTP {}", resp.status()));
        }

        let text = resp.text().await?;

        #[derive(serde::Deserialize)]
        struct SubInfo {
            list: Vec<SubItem>,
        }
        #[derive(serde::Deserialize)]
        struct SubItem {
            sub_content: String,
        }
        #[derive(serde::Deserialize)]
        struct SubContent {
            save_playback: SavePlayback,
        }
        #[derive(serde::Deserialize)]
        struct SavePlayback {
            is_m3u8: String,
            contents: String,
        }

        let info: SubInfo =
            serde_json::from_str(&text).context("解析视频元数据 JSON 失败")?;
        let item = info.list.first().context("视频元数据列表为空")?;
        let content: SubContent =
            serde_json::from_str(&item.sub_content).context("解析 sub_content 失败")?;

        if content.save_playback.is_m3u8 != "yes" {
            return Err(anyhow!(
                "不支持的视频格式（非 m3u8）: {}",
                content.save_playback.contents
            ));
        }

        Ok(content.save_playback.contents)
    }

    /// 获取视频的完整下载信息（m3u8 播放列表解析后）
    pub async fn get_video_detail(&self, video: &VideoInfo) -> Result<VideoDetail> {
        let redirect_url = self.get_video_redirect_url(&video.url).await?;
        let m3u8_url = self.get_video_m3u8_url(&redirect_url).await?;

        // 下载 m3u8 播放列表
        let resp = self
            .client
            .get(&m3u8_url)
            .send()
            .await
            .context("下载 m3u8 播放列表失败")?;
        let m3u8_raw = resp.bytes().await?;

        let (_, playlist) = m3u8_rs::parse_playlist(&m3u8_raw)
            .map_err(|e| anyhow!("解析 m3u8 失败: {e}"))
            .context("解析播放列表")?;

        let media_pl = match playlist {
            m3u8_rs::Playlist::MediaPlaylist(pl) => pl,
            m3u8_rs::Playlist::MasterPlaylist(_) => {
                return Err(anyhow!("暂不支持 Master Playlist 格式"))
            }
        };

        let base_url = url::Url::parse(&m3u8_url).context("解析 m3u8 URL 失败")?;

        Ok(VideoDetail {
            base_url,
            playlist: media_pl,
        })
    }

    /// 下载单个视频片段（原始数据，可能是加密的）
    pub async fn download_segment(&self, url: &str) -> Result<bytes::Bytes> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .context("下载视频片段失败")?;
        if !resp.status().is_success() {
            return Err(anyhow!("视频片段下载 HTTP {}", resp.status()));
        }
        Ok(resp.bytes().await?)
    }

    /// 获取 AES-128 密钥
    pub async fn get_aes_key(&self, url: &str) -> Result<[u8; 16]> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .context("获取 AES 密钥失败")?;
        let data = resp.bytes().await?.to_vec();
        if data.len() != 16 {
            return Err(anyhow!("AES 密钥长度错误: {} (预期 16)", data.len()));
        }
        let mut key = [0u8; 16];
        key.copy_from_slice(&data);
        Ok(key)
    }
}

/// 解密 AES-128-CBC 加密的视频片段
pub fn decrypt_segment(
    key: &[u8; 16],
    iv: &[u8; 16],
    data: &[u8],
) -> Result<Vec<u8>> {
    use aes::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7, generic_array::GenericArray};

    let aes_key = GenericArray::from(*key);
    let aes_iv = GenericArray::from(*iv);

    cbc::Decryptor::<aes::Aes128>::new(&aes_key, &aes_iv)
        .decrypt_padded_vec_mut::<Pkcs7>(data)
        .map_err(|e| anyhow!("AES 解密失败: {e}"))
}

/// 从 m3u8 Key 信息构建 IV
pub fn build_iv(key: &m3u8_rs::Key, media_sequence: u64, segment_index: usize) -> [u8; 16] {
    if let Some(iv_str) = &key.iv {
        let iv_str = iv_str.to_ascii_uppercase();
        if let Some(hex) = iv_str.strip_prefix("0X") {
            if let Ok(val) = u128::from_str_radix(hex, 16) {
                return val.to_be_bytes();
            }
        }
    }
    // 默认使用 media sequence number + segment index
    ((media_sequence as usize + segment_index) as u128).to_be_bytes()
}

/// 根据文件扩展名返回 MIME 类型
fn get_mime_type(extension: &str) -> &'static str {
    match extension {
        "html" | "htm" => "text/html",
        "txt" => "text/plain",
        "csv" => "text/csv",
        "json" => "application/json",
        "xml" => "application/xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "avi" => "video/x-msvideo",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "7z" => "application/x-7z-compressed",
        "rar" => "application/vnd.rar",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        _ => "application/octet-stream",
    }
}
