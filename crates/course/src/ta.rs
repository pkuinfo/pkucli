//! 助教侧接口
//!
//! 读写走 Blackboard 公开 REST（/learn/api/public/v1），会话 cookie 即可认证；
//! 写操作需要 `X-Blackboard-XSRF` 头，值取自课程页面里的 `NonceUtil.nonce.ajax`。
//! REST 在本服务器上有两处坏掉的地方，改走经典页面：
//! - 提交文件下载（bbcswebdav 404）→ `/webapps/assignment/download`
//! - 登分 + 反馈文件 → 经典评分表单 `gradeAttemptForm`
//!
//! 课堂实录不在 REST 里，发布/撤销是列表页每行的 GET 链接（publishVideo / unpublishVideo.action）。

use anyhow::{anyhow, bail, Context, Result};
use scraper::{Html, Selector};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use crate::api::{get_mime_type, CourseApi};
use crate::client::COURSE_BASE;
use crate::multipart::MultipartBuilder;

const REST: &str = "/learn/api/public/v1";
pub const PREVIEW_SUFFIX: &str = "_previewuser";

/// 课堂实录插件（PKU 定制，录像由智云课堂自动同步进来）
const VIDEO_BASE: &str = "/webapps/bb-streammedia-hqy-BBLEARN/";

pub const FOLDER: &str = "resource/x-bb-folder";
pub const ASSIGNMENT: &str = "resource/x-bb-assignment";

// ─── 数据模型 ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct Page<T> {
    results: Vec<T>,
    paging: Option<Paging>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Paging {
    next_page: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Availability {
    pub available: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    pub id: String,
    pub title: String,
    pub content_handler: Option<Handler>,
    pub availability: Option<Availability>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Handler {
    pub id: String,
}

impl Content {
    pub fn handler(&self) -> &str {
        self.content_handler.as_ref().map_or("", |h| h.id.as_str())
    }

    pub fn is_visible(&self) -> bool {
        self.availability
            .as_ref()
            .and_then(|a| a.available.as_deref())
            == Some("Yes")
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub id: String,
    pub name: String,
    pub content_id: Option<String>,
    pub score: Option<Score>,
    pub grading: Option<Grading>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Score {
    pub possible: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Grading {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub due: Option<String>,
}

impl Column {
    /// 「总计」「加权总计」等计算列，绝不写入
    pub fn is_calculated(&self) -> bool {
        self.grading.as_ref().and_then(|g| g.kind.as_deref()) == Some("Calculated")
    }

    pub fn kind(&self) -> &str {
        self.grading
            .as_ref()
            .and_then(|g| g.kind.as_deref())
            .unwrap_or("")
    }

    pub fn possible(&self) -> Option<f64> {
        self.score.as_ref().and_then(|s| s.possible)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Membership {
    user_id: String,
    course_role_id: String,
    availability: Option<Availability>,
    user: Option<User>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct User {
    user_name: String,
    name: Option<Name>,
}

#[derive(Deserialize)]
struct Name {
    given: Option<String>,
    family: Option<String>,
}

/// 选课学生（已排除预览账号）
#[derive(Debug, Clone)]
pub struct Student {
    pub user_id: String,
    /// 学号
    pub user_name: String,
    pub name: String,
    /// 院系
    pub dept: String,
    pub available: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attempt {
    pub id: String,
    pub user_id: String,
    pub status: Option<String>,
    pub score: Option<f64>,
    pub created: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AttemptFile {
    pub id: String,
    pub name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedAssignment {
    pub content_id: String,
    pub grade_column_id: String,
}

#[derive(Deserialize)]
struct Upload {
    id: String,
}

/// 课堂实录里的一节录像
#[derive(Debug, Clone)]
pub struct Recording {
    pub title: String,
    pub time: String,
    /// 智云课堂录像 ID（hqySubId）
    pub sub_id: String,
    pub published: bool,
    /// 该行的「发布」或「撤销」链接，相对 VIDEO_BASE
    toggle_href: String,
}

/// 新建作业的参数
pub struct NewAssignment<'a> {
    pub parent_id: &'a str,
    pub title: &'a str,
    pub instructions: Option<&'a str>,
    pub files: &'a [std::path::PathBuf],
    /// UTC，RFC 3339
    pub due_utc: &'a str,
    pub points: f64,
    pub attempts: u32,
    pub visible: bool,
}

/// 一次登分要写入的内容
pub struct GradeInput<'a> {
    pub score: f64,
    /// HTML 评语；None 表示保留原有评语
    pub feedback_html: Option<String>,
    pub files: &'a [std::path::PathBuf],
}

// ─── 客户端 ────────────────────────────────────────────────────

pub struct TaApi {
    api: CourseApi,
    pub course_id: String,
    xsrf: Mutex<Option<String>>,
}

impl TaApi {
    pub fn new(api: CourseApi, course_id: String) -> Self {
        Self {
            api,
            course_id,
            xsrf: Mutex::new(None),
        }
    }

    fn client(&self) -> &reqwest::Client {
        self.api.http()
    }

    fn course_path(&self, rest: &str) -> String {
        format!("{COURSE_BASE}{REST}/courses/{}/{rest}", self.course_id)
    }

    async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let resp = self
            .client()
            .get(url)
            .header("accept", "application/json")
            .send()
            .await
            .context("请求教学网 REST 接口失败")?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            bail!("HTTP {status}: {}", rest_error(&text));
        }
        serde_json::from_str(&text).with_context(|| format!("解析 REST 响应失败: {url}"))
    }

    async fn get_all<T: DeserializeOwned>(&self, url: &str) -> Result<Vec<T>> {
        let mut out = Vec::new();
        let mut next = Some(url.to_string());
        while let Some(u) = next {
            let page: Page<T> = self.get_json(&u).await?;
            out.extend(page.results);
            next = page
                .paging
                .and_then(|p| p.next_page)
                .filter(|p| !p.is_empty())
                .map(|p| format!("{COURSE_BASE}{p}"));
        }
        Ok(out)
    }

    /// 写操作用的 XSRF token（课程公告页里的 ajax nonce）
    async fn xsrf(&self) -> Result<String> {
        if let Some(t) = self.xsrf.lock().expect("xsrf lock").clone() {
            return Ok(t);
        }
        let url = format!(
            "{COURSE_BASE}/webapps/blackboard/execute/announcement\
             ?method=search&context=course_entry&course_id={}\
             &handle=announcements_entry&mode=view",
            self.course_id
        );
        let html = self.get_html(&url).await?;
        let token = find_input_value(&html, "blackboard.platform.security.NonceUtil.nonce.ajax")
            .ok_or_else(|| anyhow!("课程页面里没有找到 XSRF nonce，会话可能已失效"))?;
        *self.xsrf.lock().expect("xsrf lock") = Some(token.clone());
        Ok(token)
    }

    async fn get_html(&self, url: &str) -> Result<String> {
        let resp = self
            .client()
            .get(url)
            .send()
            .await
            .context("请求页面失败")?;
        if !resp.status().is_success() {
            bail!("页面返回 HTTP {}", resp.status());
        }
        Ok(resp.text().await?)
    }

    async fn send_json(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<String> {
        let mut req = self
            .client()
            .request(method, url)
            .header("accept", "application/json")
            .header("X-Blackboard-XSRF", self.xsrf().await?);
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req.send().await.context("REST 写请求失败")?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            bail!("HTTP {status}: {}", rest_error(&text));
        }
        Ok(text)
    }

    // ── 读 ──

    pub async fn top_folders(&self) -> Result<Vec<Content>> {
        self.get_all(&self.course_path("contents")).await
    }

    pub async fn children(&self, content_id: &str) -> Result<Vec<Content>> {
        self.get_all(&self.course_path(&format!("contents/{content_id}/children")))
            .await
    }

    pub async fn content(&self, content_id: &str) -> Result<Content> {
        self.get_json(&self.course_path(&format!("contents/{content_id}")))
            .await
    }

    pub async fn columns(&self) -> Result<Vec<Column>> {
        self.get_all(&self.course_path("gradebook/columns")).await
    }

    pub async fn column(&self, column_id: &str) -> Result<Column> {
        self.get_json(&self.course_path(&format!("gradebook/columns/{column_id}")))
            .await
    }

    /// 学生名单：只保留 Student 角色，排除 `*_previewuser` 预览账号
    pub async fn students(&self) -> Result<Vec<Student>> {
        self.members(false).await
    }

    /// 含预览账号的名单：只给显式点名预览账号的单人登分用（测试评分流程）
    pub async fn students_with_preview(&self) -> Result<Vec<Student>> {
        self.members(true).await
    }

    async fn members(&self, include_preview: bool) -> Result<Vec<Student>> {
        let members: Vec<Membership> = self
            .get_all(&self.course_path("users?limit=100&expand=user"))
            .await?;
        let mut students: Vec<Student> = members
            .into_iter()
            .filter(|m| m.course_role_id == "Student")
            .filter_map(|m| {
                let user = m.user?;
                if !include_preview && user.user_name.ends_with(PREVIEW_SUFFIX) {
                    return None;
                }
                // 北大教学网把完整姓名放在 given，family 里是院系
                let (dept, name) = user
                    .name
                    .map(|n| (n.family.unwrap_or_default(), n.given.unwrap_or_default()))
                    .unwrap_or_default();
                Some(Student {
                    user_id: m.user_id,
                    user_name: user.user_name,
                    name: name.trim().to_string(),
                    dept: dept.trim().to_string(),
                    available: m.availability.and_then(|a| a.available).as_deref() == Some("Yes"),
                })
            })
            .collect();
        students.sort_by(|a, b| a.user_name.cmp(&b.user_name));
        Ok(students)
    }

    pub async fn attempts(&self, column_id: &str) -> Result<Vec<Attempt>> {
        self.get_all(
            &self.course_path(&format!("gradebook/columns/{column_id}/attempts?limit=100")),
        )
        .await
    }

    /// 单次提交（本服务器上 `gradebook/attempts/{id}` 是 404，要走列下的路径）
    pub async fn attempt(&self, column_id: &str, attempt_id: &str) -> Result<Attempt> {
        self.get_json(&self.course_path(&format!(
            "gradebook/columns/{column_id}/attempts/{attempt_id}"
        )))
        .await
    }

    pub async fn attempt_files(&self, attempt_id: &str) -> Result<Vec<AttemptFile>> {
        self.get_all(&self.course_path(&format!("gradebook/attempts/{attempt_id}/files")))
            .await
    }

    /// 下载提交文件（REST 下载在本服务器上 404，走经典链接）
    pub async fn download_attempt_file(
        &self,
        attempt_id: &str,
        file: &AttemptFile,
    ) -> Result<Vec<u8>> {
        let mut url = url::Url::parse(&format!("{COURSE_BASE}/webapps/assignment/download"))?;
        url.query_pairs_mut()
            .append_pair("course_id", &self.course_id)
            .append_pair("attempt_id", attempt_id)
            .append_pair("file_id", &file.id)
            .append_pair("fileName", &file.name);
        let resp = self
            .client()
            .get(url)
            .send()
            .await
            .context("下载提交文件失败")?;
        if !resp.status().is_success() {
            bail!("下载 {} 失败: HTTP {}", file.name, resp.status());
        }
        if resp.url().host_str().is_some_and(|h| h.contains("iaaa")) {
            bail!("会话已失效，请重新运行 `course login`");
        }
        Ok(resp.bytes().await?.to_vec())
    }

    // ── 写 ──

    async fn upload(&self, path: &Path) -> Result<(String, String, &'static str)> {
        let filename = file_name(path)?;
        let mime = mime_of(path);
        let body = MultipartBuilder::new().add_file(
            "file",
            &filename,
            mime,
            std::fs::File::open(path)
                .with_context(|| format!("打开文件失败: {}", path.display()))?,
        );
        let boundary = body.boundary().to_owned();
        let bytes = body.build().context("构建上传表单失败")?;
        let resp = self
            .client()
            .post(format!("{COURSE_BASE}{REST}/uploads"))
            .header("accept", "application/json")
            .header("X-Blackboard-XSRF", self.xsrf().await?)
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(bytes)
            .send()
            .await
            .context("上传文件失败")?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            bail!("上传 {filename} 失败: HTTP {status}: {}", rest_error(&text));
        }
        let up: Upload = serde_json::from_str(&text).context("解析上传结果失败")?;
        Ok((up.id, filename, mime))
    }

    /// 在文件夹下新建「文件」条目
    pub async fn create_file_item(
        &self,
        parent_id: &str,
        title: &str,
        description: Option<&str>,
        path: &Path,
        visible: bool,
    ) -> Result<Content> {
        let (upload_id, filename, mime) = self.upload(path).await?;
        let mut body = serde_json::json!({
            "title": title,
            "availability": { "available": yes_no(visible) },
            "contentHandler": {
                "id": "resource/x-bb-file",
                "file": { "uploadId": upload_id, "fileName": filename, "mimeType": mime }
            }
        });
        if let Some(d) = description {
            body["body"] = serde_json::Value::String(text_to_html(d));
        }
        let text = self
            .send_json(
                reqwest::Method::POST,
                &self.course_path(&format!("contents/{parent_id}/children")),
                Some(&body),
            )
            .await?;
        serde_json::from_str(&text).context("解析新建条目结果失败")
    }

    /// 新建作业（同时生成成绩列）
    pub async fn create_assignment(&self, a: &NewAssignment<'_>) -> Result<CreatedAssignment> {
        let mut upload_ids = Vec::new();
        for f in a.files {
            upload_ids.push(self.upload(f).await?.0);
        }
        let body = serde_json::json!({
            "parentId": a.parent_id,
            "title": a.title,
            "instructions": a.instructions.map(text_to_html).unwrap_or_default(),
            "fileUploadIds": upload_ids,
            "availability": { "available": yes_no(a.visible) },
            "grading": { "due": a.due_utc, "attemptsAllowed": a.attempts },
            "score": { "possible": a.points }
        });
        let text = self
            .send_json(
                reqwest::Method::POST,
                &self.course_path("contents/createAssignment"),
                Some(&body),
            )
            .await?;
        serde_json::from_str(&text).context("解析新建作业结果失败")
    }

    /// 课堂实录（编辑模式列表，按时间排序）
    pub async fn recordings(&self) -> Result<Vec<Recording>> {
        let url = format!(
            "{COURSE_BASE}{VIDEO_BASE}videoList.action?sortDir=ASCENDING&numResults=100\
             &editPaging=false&course_id={}&mode=modify&startIndex=0",
            self.course_id
        );
        let mut list = parse_recordings(&self.get_html(&url).await?);
        list.sort_by(|a, b| a.time.cmp(&b.time));
        Ok(list)
    }

    /// 发布或撤销一节录像：请求该行的「发布」/「撤销」链接，再回读列表核对
    pub async fn set_published(&self, rec: &Recording, publish: bool) -> Result<()> {
        if rec.published == publish {
            return Ok(());
        }
        let url = url::Url::parse(&format!("{COURSE_BASE}{VIDEO_BASE}"))?.join(&rec.toggle_href)?;
        let resp = self
            .client()
            .get(url)
            .send()
            .await
            .context("请求课堂实录失败")?;
        if !resp.status().is_success() {
            bail!("「{}」操作失败: HTTP {}", rec.title, resp.status());
        }
        match self
            .recordings()
            .await?
            .iter()
            .find(|r| r.sub_id == rec.sub_id)
        {
            Some(r) if r.published == publish => Ok(()),
            Some(_) => bail!("「{}」的发布状态没有变化", rec.title),
            None => bail!("操作后列表里找不到「{}」", rec.title),
        }
    }

    /// 删除内容条目，连同它的成绩列
    pub async fn delete_content(&self, content_id: &str) -> Result<()> {
        self.send_json(
            reqwest::Method::DELETE,
            &self.course_path(&format!("contents/{content_id}?deleteGrades=true")),
            None,
        )
        .await?;
        Ok(())
    }

    /// 通过经典评分表单给一次提交登分（可附评语和反馈文件），再用 REST 回读核对
    pub async fn grade_attempt(
        &self,
        column_id: &str,
        attempt_id: &str,
        input: &GradeInput<'_>,
    ) -> Result<Attempt> {
        let page_url = format!(
            "{COURSE_BASE}/webapps/assignment/gradeAssignmentRedirector\
             ?course_id={}&outcomeDefinitionId={column_id}&attempt_id={attempt_id}\
             &source=cp_gradebook_needs_grading",
            self.course_id
        );
        let html = self.get_html(&page_url).await?;
        if html.contains("学生预览模式为“打开”") {
            bail!("当前会话处于学生预览模式，请先在网页上退出预览");
        }
        let form = parse_form(&html, "gradeAttemptForm")?;
        let submit_url = regex::Regex::new(
            r"inlineGrader\(\s*'currentAttempt',\s*'[^']*',\s*'[^']*',\s*'([^']+)'",
        )
        .expect("static regex")
        .captures(&html)
        .map(|c| c[1].to_string())
        .ok_or_else(|| anyhow!("评分页面里没有找到提交地址"))?;

        let mut over: Vec<(&str, String)> = vec![("grade", fmt_score(input.score))];
        if let Some(fb) = &input.feedback_html {
            over.push(("feedbacktext", fb.clone()));
            over.push(("feedbacktype", "H".into()));
        }
        let mut fields = with_overrides(form.fields, &over);
        let nonce_key = "blackboard.platform.security.NonceUtil.nonce";
        if !fields.iter().any(|(k, _)| k == nonce_key) {
            if let Some(n) = find_input_value(&html, nonce_key) {
                fields.push((nonce_key.into(), n));
            }
        }

        let names: Vec<(String, String)> = input
            .files
            .iter()
            .enumerate()
            .map(|(i, p)| Ok((format!("feedbackFiles_LocalFile{i}"), file_name(p)?)))
            .collect::<Result<_>>()?;
        let mut body = MultipartBuilder::new();
        for (k, v) in &fields {
            body = body.add_field(k, v.as_bytes());
        }
        for ((field, name), path) in names.iter().zip(input.files) {
            for (k, v) in [
                ("feedbackFiles_attachmentType", "L"),
                ("feedbackFiles_fileId", "new"),
                ("feedbackFiles_artifactFileId", "undefined"),
                ("feedbackFiles_artifactType", "undefined"),
                ("feedbackFiles_artifactTypeResourceKey", "undefined"),
            ] {
                body = body.add_field(k, v.as_bytes());
            }
            body = body
                .add_field("feedbackFiles_linkTitle", name.as_bytes())
                .add_file(
                    field,
                    name,
                    mime_of(path),
                    std::fs::File::open(path)
                        .with_context(|| format!("打开文件失败: {}", path.display()))?,
                );
        }
        let boundary = body.boundary().to_owned();
        let bytes = body.build().context("构建评分表单失败")?;

        let target = url::Url::parse(COURSE_BASE)?.join(&submit_url)?;
        let resp = self
            .client()
            .post(target)
            .header("origin", COURSE_BASE)
            .header("referer", &page_url)
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(bytes)
            .send()
            .await
            .context("提交评分失败")?;
        // 成功时服务器 302 到 "/"，状态码说明不了问题，以回读为准
        if resp.status().is_server_error() {
            bail!("提交评分失败: HTTP {}", resp.status());
        }

        let after = self.attempt(column_id, attempt_id).await?;
        if after.score.is_none_or(|s| (s - input.score).abs() > 1e-6) {
            bail!(
                "提交后回读分数为 {:?}，与预期 {} 不符",
                after.score,
                fmt_score(input.score)
            );
        }
        Ok(after)
    }
}

// ─── 工具函数 ──────────────────────────────────────────────────

fn rest_error(text: &str) -> String {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(String::from))
        .unwrap_or_else(|| text.chars().take(200).collect())
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "Yes"
    } else {
        "No"
    }
}

fn file_name(path: &Path) -> Result<String> {
    Ok(path
        .file_name()
        .with_context(|| format!("无效的文件路径: {}", path.display()))?
        .to_string_lossy()
        .to_string())
}

fn mime_of(path: &Path) -> &'static str {
    get_mime_type(
        &path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase(),
    )
}

pub fn fmt_score(s: f64) -> String {
    if s.fract() == 0.0 {
        format!("{s:.0}")
    } else {
        format!("{s}")
    }
}

/// 纯文本转成 Blackboard 内容编辑器能显示的 HTML（每行一段）
pub fn text_to_html(text: &str) -> String {
    text.lines()
        .map(|l| format!("<p>{}</p>", escape_html(l)))
        .collect()
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn decode_entities(s: &str) -> String {
    let re = regex::Regex::new(r"&(#x[0-9a-fA-F]+|#[0-9]+|amp|lt|gt|quot|apos|nbsp);")
        .expect("static regex");
    re.replace_all(s, |c: &regex::Captures<'_>| {
        let e = &c[1];
        match e {
            "amp" => "&".into(),
            "lt" => "<".into(),
            "gt" => ">".into(),
            "quot" => "\"".into(),
            "apos" => "'".into(),
            "nbsp" => "\u{a0}".into(),
            _ => {
                let n = if let Some(h) = e.strip_prefix("#x") {
                    u32::from_str_radix(h, 16).ok()
                } else {
                    e[1..].parse().ok()
                };
                n.and_then(char::from_u32)
                    .map(String::from)
                    .unwrap_or_else(|| c[0].to_string())
            }
        }
    })
    .into_owned()
}

/// 读 HTML 标签属性，兼容双引号、单引号和无引号写法
fn attr(tag: &str, name: &str) -> Option<String> {
    let re = regex::Regex::new(&format!(
        r#"(?i)\b{}\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#,
        regex::escape(name)
    ))
    .expect("attr regex");
    re.captures(tag).map(|c| {
        c.get(1)
            .or_else(|| c.get(2))
            .or_else(|| c.get(3))
            .map_or(String::new(), |m| m.as_str().to_string())
    })
}

/// 标签上是否有某个布尔属性（disabled、checked 等）；先去掉引号里的属性值，免得 class="disabled" 之类误判
fn has_flag(tag: &str, flag: &str) -> bool {
    let quoted = regex::Regex::new(r#""[^"]*"|'[^']*'"#).expect("static regex");
    let bare = quoted.replace_all(tag, "");
    regex::Regex::new(&format!(r"(?i)\b{flag}\b"))
        .expect("flag regex")
        .is_match(&bare)
}

/// 在整页里找某个 name 的 `<input>` 的 value（nonce 不一定在目标表单内）
fn find_input_value(html: &str, name: &str) -> Option<String> {
    let re = regex::Regex::new(r"(?i)<input\b[^>]*>").expect("static regex");
    let tag = re
        .find_iter(html)
        .map(|m| m.as_str())
        .find(|t| attr(t, "name").as_deref() == Some(name))?;
    attr(tag, "value").map(|v| decode_entities(&v))
}

struct Form {
    fields: Vec<(String, String)>,
}

/// 把 `<form>`（按 id 或 name 定位）解析成浏览器默认会提交的字段列表。
/// 用正则而不是 DOM：表单常嵌在 table 里，HTML5 解析会把 input 移出 form 节点。
/// disabled 的控件浏览器不提交，这里也必须跳过：评分页每个已有反馈文件都带一个
/// disabled 的 `feedbackFiles_remove`，误提交会把文件删掉。
fn parse_form(html: &str, key: &str) -> Result<Form> {
    let form_re = regex::Regex::new(r"(?is)<form\b[^>]*>.*?</form>").expect("static regex");
    let open_re = regex::Regex::new(r"(?is)^<form\b[^>]*>").expect("static regex");
    let f = form_re
        .find_iter(html)
        .map(|m| m.as_str())
        .find(|s| {
            let tag = open_re.find(s).map_or("", |m| m.as_str());
            attr(tag, "id").as_deref() == Some(key) || attr(tag, "name").as_deref() == Some(key)
        })
        .ok_or_else(|| anyhow!("页面里没有找到表单 {key}"))?;

    let mut fields = Vec::new();
    let input_re = regex::Regex::new(r"(?i)<input\b[^>]*>").expect("static regex");
    for m in input_re.find_iter(f) {
        let t = m.as_str();
        let Some(name) = attr(t, "name") else {
            continue;
        };
        if has_flag(t, "disabled") {
            continue;
        }
        let ty = attr(t, "type")
            .unwrap_or_else(|| "text".into())
            .to_lowercase();
        if ["button", "submit", "file", "image", "reset"].contains(&ty.as_str()) {
            continue;
        }
        if (ty == "checkbox" || ty == "radio") && !has_flag(t, "checked") {
            continue;
        }
        let value = attr(t, "value").unwrap_or_else(|| {
            if ty == "checkbox" {
                "on".into()
            } else {
                String::new()
            }
        });
        fields.push((name, decode_entities(&value)));
    }
    let ta_re =
        regex::Regex::new(r"(?is)(<textarea\b[^>]*>)(.*?)</textarea>").expect("static regex");
    for c in ta_re.captures_iter(f) {
        if has_flag(&c[1], "disabled") {
            continue;
        }
        if let Some(name) = attr(&c[1], "name") {
            fields.push((name, decode_entities(&c[2])));
        }
    }
    let sel_re = regex::Regex::new(r"(?is)(<select\b[^>]*>)(.*?)</select>").expect("static regex");
    let opt_re = regex::Regex::new(r"(?i)<option\b[^>]*>").expect("static regex");
    for c in sel_re.captures_iter(f) {
        let Some(name) = attr(&c[1], "name") else {
            continue;
        };
        if has_flag(&c[1], "multiple") || has_flag(&c[1], "disabled") {
            continue;
        }
        let opts: Vec<&str> = opt_re.find_iter(&c[2]).map(|m| m.as_str()).collect();
        let pick = opts
            .iter()
            .find(|o| has_flag(o, "selected"))
            .or(opts.first());
        let value = pick.and_then(|o| attr(o, "value")).unwrap_or_default();
        fields.push((name, decode_entities(&value)));
    }
    Ok(Form { fields })
}

/// 解析课堂实录列表：已发布的行有「撤销」链接，未发布的行有「发布」链接
fn parse_recordings(html: &str) -> Vec<Recording> {
    let dom = Html::parse_document(html);
    let row_sel = Selector::parse("tbody#listContainer_databody > tr").expect("static selector");
    let th_sel = Selector::parse("th").expect("static selector");
    let time_sel = Selector::parse("span.table-data-cell-value").expect("static selector");
    let a_sel = Selector::parse("a").expect("static selector");
    let sub_re = regex::Regex::new(r"hqySubId=(\d+)").expect("static regex");
    dom.select(&row_sel)
        .filter_map(|tr| {
            let title = tr.select(&th_sel).next()?.text().collect::<String>();
            let time = tr
                .select(&time_sel)
                .next()
                .map(|el| el.text().collect::<String>())
                .unwrap_or_default();
            let (published, href) = tr.select(&a_sel).find_map(|a| {
                let href = a.value().attr("href")?;
                match a.text().collect::<String>().trim() {
                    "撤销" => Some((true, href)),
                    "发布" => Some((false, href)),
                    _ => None,
                }
            })?;
            Some(Recording {
                title: title.trim().to_string(),
                time: time.trim().to_string(),
                sub_id: sub_re.captures(href)?[1].to_string(),
                published,
                toggle_href: href.to_string(),
            })
        })
        .collect()
}

/// 用覆盖值替换同名字段（同名的全部删掉再追加）
fn with_overrides(fields: Vec<(String, String)>, over: &[(&str, String)]) -> Vec<(String, String)> {
    let keys: Vec<&str> = over.iter().map(|(k, _)| *k).collect();
    let mut out: Vec<(String, String)> = fields
        .into_iter()
        .filter(|(k, _)| !keys.contains(&k.as_str()))
        .collect();
    out.extend(over.iter().map(|(k, v)| (k.to_string(), v.clone())));
    out
}

/// 按学号建索引，方便把 attempt 的 userId 映射回学生
pub fn index_by_user_id(students: &[Student]) -> HashMap<&str, &Student> {
    students.iter().map(|s| (s.user_id.as_str(), s)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_parse_handles_quotes_checkboxes_and_tables() {
        let html = r#"<table><form id="f" name='gradeAttemptForm' action="/x">
            <tr><td><input type="hidden" name='nonce' value='a&amp;b'>
            <input name="grade" value="">
            <input type="checkbox" name="c1" checked><input type="checkbox" name="c2">
            <input type="submit" name="go" value="提交">
            <textarea name="feedbacktext">&lt;p&gt;hi&lt;/p&gt;</textarea>
            <select name="s"><option value="1">a<option value="2" selected>b</select>
            </td></tr></form></table>"#;
        let f = parse_form(html, "gradeAttemptForm").unwrap();
        let get = |k: &str| {
            f.fields
                .iter()
                .find(|(n, _)| n == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("nonce"), Some("a&b"));
        assert_eq!(get("grade"), Some(""));
        assert_eq!(get("c1"), Some("on"));
        assert_eq!(get("c2"), None);
        assert_eq!(get("go"), None);
        assert_eq!(get("feedbacktext"), Some("<p>hi</p>"));
        assert_eq!(get("s"), Some("2"));
    }

    #[test]
    fn form_parse_skips_disabled_controls() {
        let html = r#"<form id="gradeAttemptForm">
            <input type="hidden" name="feedbackFiles_remove" value="_3688825_1" disabled>
            <input type="hidden" name="keep" value="1">
            <textarea name="t" disabled="disabled">x</textarea>
            <select name="s" DISABLED><option value="1" selected>a</select>
            </form>"#;
        let f = parse_form(html, "gradeAttemptForm").unwrap();
        assert_eq!(f.fields, vec![("keep".to_string(), "1".to_string())]);
    }

    #[test]
    fn flag_ignores_words_inside_attribute_values() {
        assert!(has_flag("<input name='a' disabled>", "disabled"));
        assert!(has_flag("<input disabled=\"disabled\">", "disabled"));
        assert!(!has_flag(
            "<input class=\"disabled\" value='checked'>",
            "disabled"
        ));
        assert!(!has_flag(
            "<input class=\"disabled\" value='checked'>",
            "checked"
        ));
    }

    #[test]
    fn overrides_replace_all_same_name_fields() {
        let fields = vec![
            ("a".into(), "1".into()),
            ("b".into(), "2".into()),
            ("a".into(), "3".into()),
        ];
        let out = with_overrides(fields, &[("a", "9".into())]);
        assert_eq!(
            out,
            vec![("b".to_string(), "2".to_string()), ("a".into(), "9".into())]
        );
    }

    #[test]
    fn nonce_lookup_accepts_single_quotes() {
        let html = "<input type='hidden' name='blackboard.platform.security.NonceUtil.nonce' value='abc-123'>";
        assert_eq!(
            find_input_value(html, "blackboard.platform.security.NonceUtil.nonce").as_deref(),
            Some("abc-123")
        );
    }

    #[test]
    fn text_to_html_escapes_and_splits_lines() {
        assert_eq!(text_to_html("a<b\n第二行"), "<p>a&lt;b</p><p>第二行</p>");
    }

    #[test]
    fn recordings_parse_publish_state() {
        let html = r#"<table><tbody id="listContainer_databody">
            <tr id="listContainer_row:0"><th scope="row">2026-09-07第5-6节</th>
              <td><span class="table-data-cell-value">2026-09-07 13:00:00</span></td>
              <td><span class="table-data-cell-value">教师</span></td>
              <td><span class="table-data-cell-value">
                <a href="playVideo.action?token=x">预览</a>
                <a class="inlineAction" href="publishVideo.action?kcwybm=k&amp;course_id=_1_1&amp;hqySubId=4612154">发布</a>
              </span></td></tr>
            <tr id="listContainer_row:1"><th scope="row">2026-10-21第1-2节</th>
              <td><span class="table-data-cell-value">2026-10-21 08:00:00</span></td>
              <td><span class="table-data-cell-value">
                <a href="unpublishVideo.action?kcwybm=k&amp;course_id=_1_1&amp;hqySubId=4612242">撤销</a>
              </span></td></tr>
            </tbody></table>"#;
        let list = parse_recordings(html);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].sub_id, "4612154");
        assert!(!list[0].published);
        assert_eq!(
            list[0].toggle_href,
            "publishVideo.action?kcwybm=k&course_id=_1_1&hqySubId=4612154"
        );
        assert_eq!(list[1].time, "2026-10-21 08:00:00");
        assert!(list[1].published);
    }

    #[test]
    fn score_format() {
        assert_eq!(fmt_score(88.0), "88");
        assert_eq!(fmt_score(87.5), "87.5");
    }
}
