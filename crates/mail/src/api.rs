//! 邮件 RPC（`POST https://<mailhost>/js6/s?func=…&sid=…`）和通讯录 REST
//!
//! 会话失效（`FA_INVALID_SESSION`、302 跳转、返回网页）时，用 keyring / 环境变量里的凭据
//! 自动重新登录一次再重试；设备未被信任（需要短信）时提示用户手动登录。

use crate::client;
use crate::login::{self, MailSession, APP_NAME};
use crate::parse::{self, addr_list, Js6Error};
use anyhow::{anyhow, bail, Context, Result};
use colored::Colorize;
use pkuinfo_common::session::Store;
use reqwest_cookie_store::CookieStoreMutex;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;

/// 系统文件夹 id
pub const FID_INBOX: i64 = 1;
pub const FID_DRAFTS: i64 = 2;
pub const FID_SENT: i64 = 3;
pub const FID_TRASH: i64 = 4;
pub const FID_SPAM: i64 = 5;

fn str_of(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn int_of(v: Option<&Value>) -> i64 {
    match v {
        Some(Value::Number(n)) => n.as_i64().unwrap_or_default(),
        Some(Value::String(s)) => s.parse().unwrap_or_default(),
        _ => 0,
    }
}

fn flag(v: &Value, name: &str) -> bool {
    match v.get("flags").and_then(|f| f.get(name)) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_i64().unwrap_or_default() != 0,
        _ => false,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Folder {
    pub id: i64,
    pub name: String,
    pub total: i64,
    pub unread: i64,
}

impl Folder {
    fn from_value(v: &Value) -> Self {
        let stats = v.get("stats");
        Self {
            id: int_of(v.get("id")),
            name: str_of(v.get("name")),
            total: int_of(stats.and_then(|s| s.get("messageCount"))),
            unread: int_of(stats.and_then(|s| s.get("unreadMessageCount"))),
        }
    }
}

/// 列表 / 搜索里的一封邮件
#[derive(Debug, Clone, Serialize)]
pub struct MsgSummary {
    pub id: String,
    pub fid: i64,
    pub subject: String,
    pub from: String,
    pub to: Vec<String>,
    /// 北京时间 `YYYY-MM-DD HH:mm:ss`
    pub date: String,
    pub size: i64,
    pub read: bool,
    pub attached: bool,
    pub summary: String,
}

impl MsgSummary {
    pub fn from_value(v: &Value) -> Self {
        let date = [v.get("receivedDate"), v.get("sentDate")]
            .into_iter()
            .map(str_of)
            .find(|s| !s.is_empty())
            .unwrap_or_default();
        Self {
            id: str_of(v.get("id")),
            fid: int_of(v.get("fid")),
            subject: str_of(v.get("subject")),
            from: addr_list(v.get("from")).join(", "),
            to: addr_list(v.get("to")),
            date,
            size: int_of(v.get("size")),
            read: flag(v, "read"),
            attached: flag(v, "attached") || flag(v, "realAttached"),
            summary: str_of(v.get("summary")).trim().to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Attachment {
    /// 附件 part id，`mail download <mid> <part>` 用
    pub id: String,
    pub filename: String,
    pub content_type: String,
    pub size: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageDetail {
    pub id: String,
    pub subject: String,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub date: String,
    pub attachments: Vec<Attachment>,
    /// 纯文本正文（没有 text 部分时由 HTML 转换）
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
}

impl MessageDetail {
    pub fn from_value(id: &str, v: &Value, keep_html: bool) -> Self {
        let html = v
            .get("html")
            .and_then(|h| h.get("content"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let text = v
            .get("text")
            .and_then(|t| t.get("content"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let text = if text.is_empty() {
            parse::html_to_text(html)
        } else {
            text.to_string()
        };
        let attachments = v
            .get("attachments")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|x| Attachment {
                        id: str_of(x.get("id")),
                        filename: str_of(x.get("filename")),
                        content_type: str_of(x.get("contentType")),
                        size: int_of(x.get("estimateSize").or(x.get("size"))),
                    })
                    .filter(|x| !x.id.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let date = [v.get("sentDate"), v.get("receivedDate")]
            .into_iter()
            .map(str_of)
            .find(|s| !s.is_empty())
            .unwrap_or_default();
        Self {
            id: id.to_string(),
            subject: str_of(v.get("subject")),
            from: addr_list(v.get("from")).join(", "),
            to: addr_list(v.get("to")),
            cc: addr_list(v.get("cc")),
            date,
            attachments,
            text,
            html: keep_html.then(|| html.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Contact {
    pub name: String,
    pub email: String,
    pub pinyin: String,
    pub mobile: Vec<String>,
    pub remark: String,
    pub org: String,
}

impl Contact {
    fn from_value(v: &Value) -> Self {
        let mobile = v
            .get("mobileList")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|x| str_of(Some(x)))
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let org = [v.get("orgname"), v.get("org")]
            .into_iter()
            .map(str_of)
            .find(|s| !s.is_empty())
            .unwrap_or_default();
        Self {
            name: str_of(v.get("qiyeAccountName")),
            email: str_of(v.get("email")),
            pinyin: str_of(v.get("pinyinName")),
            mobile,
            remark: str_of(v.get("remark")),
            org,
        }
    }

    pub fn matches(&self, query: &str) -> bool {
        let q = query.to_lowercase();
        [&self.name, &self.email, &self.pinyin, &self.remark]
            .iter()
            .any(|f| f.to_lowercase().contains(&q))
            || self
                .pinyin
                .split_whitespace()
                .filter_map(|w| w.chars().next())
                .collect::<String>()
                .to_lowercase()
                .contains(&q)
    }
}

pub struct MailApi {
    store: Store,
    cookies: Arc<CookieStoreMutex>,
    client: reqwest::Client,
    pub sess: MailSession,
    relogged: bool,
}

impl MailApi {
    pub fn open() -> Result<Self> {
        let store = Store::new(APP_NAME)?;
        let sess = login::load_session(&store)?
            .ok_or_else(|| anyhow!("未登录，请先运行 `mail login -p`"))?;
        let cookies = store.load_cookie_store()?;
        let client = client::build_no_redirect(cookies.clone())?;
        Ok(Self {
            store,
            cookies,
            client,
            sess,
            relogged: false,
        })
    }

    fn base(&self) -> String {
        format!("https://{}", self.sess.host)
    }

    /// 会话失效后重新登录一次；第二次失效直接报错
    async fn relogin(&mut self, why: &str) -> Result<()> {
        if self.relogged {
            bail!("邮箱会话失效（{why}），重新登录后仍然失败");
        }
        eprintln!("{} 邮箱会话失效（{why}），自动重新登录...", "[auth]".cyan());
        self.relogged = true;
        self.sess = login::relogin_silently(&self.store, &self.cookies).await?;
        Ok(())
    }

    async fn post_js6(
        &self,
        func: &str,
        extra: &[(&str, &str)],
        body: &Value,
    ) -> Result<std::result::Result<Value, Js6Error>> {
        let mut query = vec![("func", func), ("sid", self.sess.sid.as_str())];
        query.extend_from_slice(extra);
        let resp = self
            .client
            .post(format!("{}/js6/s", self.base()))
            .query(&query)
            .header("accept", "application/json")
            .json(body)
            .send()
            .await
            .with_context(|| format!("请求 {func} 失败"))?;
        let status = resp.status().as_u16();
        let text = resp
            .text()
            .await
            .with_context(|| format!("读取 {func} 响应失败"))?;
        Ok(parse::parse_js6(status, &text))
    }

    /// 调用一个 js6 函数，返回整个响应 JSON（`var` 是结果）
    pub async fn call(&mut self, func: &str, extra: &[(&str, &str)], body: Value) -> Result<Value> {
        let mut result = self.post_js6(func, extra, &body).await?;
        if let Err(Js6Error::InvalidSession(why)) = &result {
            let why = why.clone();
            self.relogin(&why).await?;
            result = self.post_js6(func, extra, &body).await?;
        }
        let v = result.map_err(|e| anyhow!("{func}：{e}"))?;
        self.store.save_cookie_store(&self.cookies)?;
        Ok(v)
    }

    pub async fn folders(&mut self) -> Result<Vec<Folder>> {
        let v = self
            .call(
                "mbox:getAllFolders",
                &[],
                json!({"order": "custom_virtual"}),
            )
            .await?;
        Ok(v["var"]
            .as_array()
            .map(|a| a.iter().map(Folder::from_value).collect())
            .unwrap_or_default())
    }

    /// 返回（本页邮件，总数）
    pub async fn list(
        &mut self,
        fid: i64,
        start: usize,
        limit: usize,
        unread_only: bool,
    ) -> Result<(Vec<MsgSummary>, i64)> {
        let filter = if unread_only {
            json!({"flags": {"read": false}})
        } else {
            json!({})
        };
        let v = self
            .call(
                "mbox:listMessages",
                &[],
                json!({
                    "fid": fid,
                    "start": start,
                    "limit": limit,
                    "order": "date",
                    "desc": true,
                    "returnTotal": true,
                    "returnTid": true,
                    "returnTag": true,
                    "returnAttachments": true,
                    "summaryWindowSize": limit,
                    "skipLockedFolders": false,
                    "topFlag": "top",
                    "filter": filter,
                }),
            )
            .await?;
        Ok(summaries(&v))
    }

    pub async fn read(&mut self, mid: &str, mark_read: bool) -> Result<Value> {
        let v = self
            .call(
                "mbox:readMessage",
                &[],
                json!({
                    "id": mid,
                    "mode": "both",
                    "markRead": mark_read,
                    "filterStylesheets": false,
                    "returnImageInfo": true,
                    "supportTNEF": true,
                    "autoName": true,
                }),
            )
            .await?;
        Ok(v["var"].clone())
    }

    pub async fn search(
        &mut self,
        pattern: &str,
        fids: &[i64],
        start: usize,
        window: usize,
    ) -> Result<(Vec<MsgSummary>, i64)> {
        let v = self
            .call(
                "mbox:searchMessages",
                &[],
                json!({
                    "pattern": pattern,
                    "fts.ext": true,
                    "fts.fields": "from,to,subj,cont,aname",
                    "conditions": [],
                    "operator": "and",
                    "order": "date",
                    "desc": true,
                    "start": start,
                    "windowSize": window,
                    "summaryWindowSize": window,
                    "returnAttachments": true,
                    "returnTotal": true,
                    "returnTag": true,
                    "fids": fids,
                    "groupings": {
                        "fid": "",
                        "flags.read": "",
                        "sentDate": "",
                        "flags.attached": "",
                        "fromAddress": "",
                    },
                }),
            )
            .await?;
        Ok(summaries(&v))
    }

    /// 发信（不可撤回！调用方必须已经得到用户确认）
    pub async fn compose(&mut self, compose_id: Option<&str>, attrs: Value) -> Result<Value> {
        let mut body = json!({
            "returnInfo": true,
            "action": "deliver",
            "attrs": attrs,
        });
        if let Some(id) = compose_id {
            body["id"] = json!(id);
        }
        let v = self
            .call(
                "mbox:compose",
                &[("l", "compose"), ("action", "deliver")],
                body,
            )
            .await?;
        Ok(v["var"].clone())
    }

    /// 回复第 1 步：服务器生成回复草稿（收件人、Re: 主题、引用原文），不发送
    pub async fn reply_prepare(&mut self, mid: &str, to_all: bool) -> Result<Value> {
        let v = self
            .call(
                "mbox:replyMessage",
                &[],
                json!({"id": mid, "toAll": to_all, "withAttachments": false}),
            )
            .await?;
        Ok(v["var"].clone())
    }

    /// 下载附件，返回响应（调用方流式写盘）
    pub async fn attachment(&mut self, mid: &str, part: &str) -> Result<reqwest::Response> {
        for _ in 0..2 {
            let resp = self
                .client
                .get(format!("{}/js6/s", self.base()))
                .query(&[
                    ("func", "mbox:getMessageData"),
                    ("sid", self.sess.sid.as_str()),
                    ("mid", mid),
                    ("part", part),
                    ("mode", "download"),
                ])
                .send()
                .await
                .context("请求附件失败")?;
            let status = resp.status().as_u16();
            let ctype = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let has_disposition = resp.headers().contains_key("content-disposition");
            let looks_error = (300..400).contains(&status)
                || (!has_disposition && (ctype.contains("json") || ctype.contains("text/html")));
            if !looks_error && resp.status().is_success() {
                return Ok(resp);
            }
            let body = resp.text().await.unwrap_or_default();
            match parse::parse_js6(status, &body) {
                Err(Js6Error::InvalidSession(why)) => self.relogin(&why).await?,
                Err(e) => bail!("下载附件失败：{e}"),
                Ok(_) => bail!("下载附件失败（HTTP {status}）"),
            }
        }
        bail!("下载附件失败")
    }

    pub async fn contacts(&mut self) -> Result<Vec<Contact>> {
        for _ in 0..2 {
            let resp = self
                .client
                .get(format!(
                    "{}/cowork/api/biz/person/personContactList",
                    self.base()
                ))
                .query(&[
                    ("email", self.sess.account.as_str()),
                    ("lastUpdateTime", "0"),
                ])
                .header("accept", "application/json")
                .send()
                .await
                .context("请求通讯录失败")?;
            let status = resp.status().as_u16();
            let body = resp.text().await.context("读取通讯录失败")?;
            let v: Option<Value> = serde_json::from_str(body.trim()).ok();
            if let Some(list) = v
                .as_ref()
                .and_then(|v| v.pointer("/data/personContactVOList"))
                .and_then(Value::as_array)
            {
                self.store.save_cookie_store(&self.cookies)?;
                return Ok(list.iter().map(Contact::from_value).collect());
            }
            if (300..400).contains(&status)
                || matches!(status, 401 | 403)
                || body.trim_start().starts_with('<')
            {
                self.relogin(&format!("通讯录 HTTP {status}")).await?;
                continue;
            }
            bail!("通讯录返回格式异常：{}", parse::snippet(&body));
        }
        bail!("读取通讯录失败")
    }
}

fn summaries(v: &Value) -> (Vec<MsgSummary>, i64) {
    let list: Vec<MsgSummary> = v["var"]
        .as_array()
        .map(|a| a.iter().map(MsgSummary::from_value).collect())
        .unwrap_or_default();
    let total = v
        .get("total")
        .map(|t| int_of(Some(t)))
        .unwrap_or(list.len() as i64);
    (list, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_summary_and_detail() {
        let v = json!({
            "code": "S_OK",
            "total": 120,
            "var": [{
                "id": "1tbiAQAAB", "fid": 1, "subject": "作业通知",
                "from": "\"张老师\" <zls@pku.edu.cn>", "to": "zs@stu.pku.edu.cn",
                "sentDate": "2026-11-17 09:00:00", "receivedDate": "2026-11-17 09:00:02",
                "size": 2048, "summary": " 请于周五前提交 ",
                "flags": {"read": false, "attached": true}
            }]
        });
        let (list, total) = summaries(&v);
        assert_eq!(total, 120);
        let m = &list[0];
        assert_eq!(m.id, "1tbiAQAAB");
        assert_eq!(m.date, "2026-11-17 09:00:02");
        assert_eq!(m.to, vec!["zs@stu.pku.edu.cn"]);
        assert!(!m.read && m.attached);
        assert_eq!(m.summary, "请于周五前提交");

        let detail = json!({
            "subject": "作业通知", "from": ["\"张老师\" <zls@pku.edu.cn>"],
            "to": ["zs@stu.pku.edu.cn"], "sentDate": "2026-11-17 09:00:00",
            "html": {"content": "<p>请于<b>周五</b>前提交</p>"},
            "attachments": [{"id": 2, "filename": "作业.pdf", "contentType": "application/pdf", "estimateSize": 1000}]
        });
        let d = MessageDetail::from_value("1tbiAQAAB", &detail, false);
        assert_eq!(d.text, "请于周五前提交");
        assert_eq!(d.attachments[0].id, "2");
        assert!(d.html.is_none());
    }

    #[test]
    fn contact_matching() {
        let c = Contact::from_value(&json!({
            "email": "zs@stu.pku.edu.cn", "qiyeAccountName": "张三",
            "pinyinName": "zhang san", "mobileList": ["13800000000"]
        }));
        assert!(c.matches("张"));
        assert!(c.matches("ZHANG"));
        assert!(c.matches("zs"));
        assert!(!c.matches("李"));
        assert_eq!(c.mobile, vec!["13800000000"]);
    }
}
