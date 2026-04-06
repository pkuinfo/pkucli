//! 教学网 API 客户端
//!
//! 基于 Blackboard Learn 平台，通过 HTML 解析获取课程信息。
//! 参考 pku3b 项目实现。

use anyhow::{anyhow, Context, Result};
use scraper::{Html, Selector};

use crate::client::{self, COURSE_BASE};
use info_common::session::Store;

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

/// 课程内容项
#[derive(Debug, Clone)]
pub struct ContentItem {
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
}

/// 内容类型
#[derive(Debug, Clone)]
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
    /// 截止时间
    pub deadline: Option<String>,
    /// 说明文本
    pub instructions: String,
    /// 附件
    pub attachments: Vec<Attachment>,
    /// 提交状态
    pub status: String,
}

// ─── API 客户端 ────────────────────────────────────────────────

pub struct CourseApi {
    client: reqwest::Client,
}

impl CourseApi {
    /// 从已保存的会话创建 API 客户端
    pub fn from_session() -> Result<Self> {
        let store = Store::new(APP_NAME)?;
        let _session = store
            .load_session()?
            .ok_or_else(|| anyhow!("未登录，请先运行 `course login`"))?;

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

        if !resp.status().is_success() {
            return Err(anyhow!("教学网主页返回 HTTP {}", resp.status()));
        }

        let body = resp.text().await?;
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

        let item_sel = Selector::parse("li.clearfix").unwrap();
        let title_sel = Selector::parse("h3").unwrap();
        let link_sel = Selector::parse("h3 a").unwrap();
        let details_sel = Selector::parse("div.details").unwrap();
        let attach_sel = Selector::parse("ul.attachments li a").unwrap();

        let mut items = Vec::new();

        for li in dom.select(&item_sel) {
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

            let description = li
                .select(&details_sel)
                .next()
                .map(|d| d.text().collect::<String>())
                .unwrap_or_default()
                .trim()
                .to_string();

            let attachments: Vec<Attachment> = li
                .select(&attach_sel)
                .filter_map(|a| {
                    let name = a.text().collect::<String>().trim().to_string();
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

            // 判断内容类型
            let item_type = if url
                .as_deref()
                .is_some_and(|u| u.contains("uploadAssignment") || u.contains("assignment"))
            {
                ContentType::Assignment
            } else if url
                .as_deref()
                .is_some_and(|u| u.contains("listContent"))
            {
                ContentType::Folder
            } else {
                ContentType::Document
            };

            items.push(ContentItem {
                title,
                item_type,
                url,
                attachments,
                description,
            });
        }

        Ok(items)
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
}
