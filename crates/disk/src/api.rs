//! AnyShare 7 文档接口
//!
//! - 顶层「文档库」：`GET /api/efast/v1/entry-doc-lib`，按 type 区分个人、其他（他人共享给我）、部门、
//!   自定义、知识库，不带 type 参数时全部返回
//! - 列目录：`POST /api/efast/v1/dir/list`，返回 `dirs` 和 `files` 两组
//! - 下载：`POST /api/efast/v1/file/osdownload` 拿到对象存储的预签名地址，再直接 GET
//!
//! 鉴权用 `Authorization: Bearer <client.oauth2_token>`。访问令牌 1 小时过期，遇到 401 时请求
//! `/anyshare/oauth2/login/refreshToken?isforced=true`（网页端同款），服务端会写回新的令牌 cookie。

use anyhow::{anyhow, bail, Context, Result};
use futures::StreamExt;
use reqwest_cookie_store::CookieStoreMutex;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

use crate::client::{self, DISK_BASE};
use crate::login::{ACCESS_COOKIE, APP_NAME};
use pkuinfo_common::session::Store;

/// 文档库类型及网页端的中文叫法，顺序即显示顺序
pub const LIB_TYPES: [(&str, &str); 5] = [
    ("user_doc_lib", "个人文档库"),
    ("shared_user_doc_lib", "其他文档库"),
    ("department_doc_lib", "部门文档库"),
    ("custom_doc_lib", "自定义文档库"),
    ("knowledge_doc_lib", "知识库"),
];

/// 文档库类型的中文名；未知类型原样返回，保证不会被漏掉
pub fn lib_label(kind: &str) -> &str {
    LIB_TYPES
        .iter()
        .find(|(k, _)| *k == kind)
        .map_or(kind, |(_, v)| v)
}

fn lib_rank(kind: &str) -> usize {
    LIB_TYPES
        .iter()
        .position(|(k, _)| *k == kind)
        .unwrap_or(LIB_TYPES.len())
}

/// 顶层文档库
#[derive(Debug, Clone, Deserialize)]
pub struct DocLib {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
}

/// 目录里的一项（文件夹的 size 为 -1）
#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    pub docid: String,
    pub name: String,
    pub size: i64,
    /// 修改时间，微秒时间戳
    pub modified: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Listing {
    #[serde(default)]
    pub dirs: Vec<Entry>,
    #[serde(default)]
    pub files: Vec<Entry>,
}

#[derive(Deserialize)]
struct OsDownload {
    authrequest: Vec<String>,
}

#[derive(Deserialize)]
struct RefreshResp {
    code: Option<i64>,
    message: Option<String>,
}

pub struct DiskApi {
    store: Store,
    cookies: Arc<CookieStoreMutex>,
    client: reqwest::Client,
}

impl DiskApi {
    pub fn from_session() -> Result<Self> {
        let store = Store::new(APP_NAME)?;
        if store.load_session()?.is_none() {
            bail!("未登录，请先运行 `disk login -p`");
        }
        let cookies = store.load_cookie_store()?;
        let client = client::build(cookies.clone())?;
        Ok(Self {
            store,
            cookies,
            client,
        })
    }

    fn access_token(&self) -> Result<String> {
        self.cookies
            .lock()
            .map_err(|_| anyhow!("锁定 cookie store 失败"))?
            .iter_any()
            .find(|c| c.name() == ACCESS_COOKIE)
            .map(|c| c.value().to_string())
            .filter(|v| !v.is_empty())
            .ok_or_else(|| anyhow!("本地没有网盘令牌，请重新运行 `disk login -p`"))
    }

    /// 用刷新令牌换新的访问令牌（会作废旧的刷新令牌，所以换完立即落盘）
    async fn refresh(&self) -> Result<()> {
        let resp = self
            .client
            .get(format!(
                "{DISK_BASE}/anyshare/oauth2/login/refreshToken?isforced=true"
            ))
            .send()
            .await
            .context("续期网盘令牌失败")?;
        let status = resp.status();
        let body: Option<RefreshResp> = resp.json().await.ok();
        let ok = status.is_success() && body.as_ref().and_then(|b| b.code) == Some(200);
        if !ok {
            bail!(
                "网盘登录已失效（续期返回 HTTP {} {}），请重新运行 `disk login -p`",
                status.as_u16(),
                body.and_then(|b| b.message).unwrap_or_default()
            );
        }
        self.store.save_cookie_store(&self.cookies)
    }

    /// 发一个 JSON 请求；访问令牌过期（401）时续期一次再重试
    async fn call<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<T> {
        for attempt in 0..2 {
            let mut req = self
                .client
                .request(method.clone(), format!("{DISK_BASE}{path}"))
                .bearer_auth(self.access_token()?)
                .header("accept", "application/json");
            if let Some(b) = body {
                req = req.json(b);
            }
            let resp = req.send().await.context("请求北大网盘失败")?;
            let status = resp.status();
            if status == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                self.refresh().await?;
                continue;
            }
            let text = resp.text().await.context("读取网盘响应失败")?;
            if !status.is_success() {
                bail!("网盘返回 HTTP {}: {}", status.as_u16(), api_error(&text));
            }
            return serde_json::from_str(&text)
                .with_context(|| format!("解析网盘响应失败: {path}"));
        }
        bail!("网盘令牌续期后仍然 401，请重新运行 `disk login -p`")
    }

    /// 全部顶层文档库，按 个人 → 其他 → 部门 → 自定义 → 知识库 → 未知类型 排序
    pub async fn libraries(&self) -> Result<Vec<DocLib>> {
        let mut libs: Vec<DocLib> = self
            .call(reqwest::Method::GET, "/api/efast/v1/entry-doc-lib", None)
            .await?;
        libs.sort_by(|a, b| {
            lib_rank(&a.kind)
                .cmp(&lib_rank(&b.kind))
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(libs)
    }

    pub async fn list(&self, docid: &str) -> Result<Listing> {
        self.call(
            reqwest::Method::POST,
            "/api/efast/v1/dir/list",
            Some(&serde_json::json!({ "docid": docid, "sort": "asc", "by": "name" })),
        )
        .await
    }

    /// 下载一个文件到 dest（先写 .part，完成后改名），返回字节数
    pub async fn download_to(&self, docid: &str, dest: &Path) -> Result<u64> {
        let os: OsDownload = self
            .call(
                reqwest::Method::POST,
                "/api/efast/v1/file/osdownload",
                Some(&serde_json::json!({ "docid": docid, "authtype": "QUERY_STRING" })),
            )
            .await?;
        // authrequest = [方法, URL, "Header: value", ...]
        let (method, url) = match os.authrequest.as_slice() {
            [m, u, ..] => (m.as_str(), u.as_str()),
            _ => bail!("网盘没有返回下载地址"),
        };
        let mut req = self
            .client
            .request(method.parse().context("下载方法无效")?, url);
        for h in os.authrequest.iter().skip(2) {
            if let Some((k, v)) = h.split_once(':') {
                req = req.header(k.trim(), v.trim());
            }
        }
        let resp = req.send().await.context("下载请求失败")?;
        if !resp.status().is_success() {
            bail!("下载失败: HTTP {}", resp.status().as_u16());
        }

        if let Some(dir) = dest.parent() {
            tokio::fs::create_dir_all(dir)
                .await
                .with_context(|| format!("创建目录失败: {}", dir.display()))?;
        }
        let part = dest.with_extension(match dest.extension() {
            Some(e) => format!("{}.part", e.to_string_lossy()),
            None => "part".into(),
        });
        let mut file = tokio::fs::File::create(&part)
            .await
            .with_context(|| format!("创建文件失败: {}", part.display()))?;
        let mut stream = resp.bytes_stream();
        let mut written = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("下载中断")?;
            file.write_all(&chunk).await?;
            written += chunk.len() as u64;
        }
        file.flush().await?;
        drop(file);
        tokio::fs::rename(&part, dest)
            .await
            .with_context(|| format!("保存文件失败: {}", dest.display()))?;
        Ok(written)
    }
}

fn api_error(text: &str) -> String {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| {
            let msg = v.get("message").and_then(|m| m.as_str())?.to_string();
            Some(match v.get("cause").and_then(|c| c.as_str()) {
                Some(c) if !c.is_empty() && c != msg => format!("{msg}（{c}）"),
                _ => msg,
            })
        })
        .unwrap_or_else(|| text.chars().take(200).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_library_types_are_kept_and_sorted_last() {
        assert_eq!(lib_label("shared_user_doc_lib"), "其他文档库");
        assert_eq!(lib_label("brand_new_lib"), "brand_new_lib");
        assert!(lib_rank("knowledge_doc_lib") < lib_rank("brand_new_lib"));
        assert!(lib_rank("user_doc_lib") < lib_rank("department_doc_lib"));
    }

    #[test]
    fn listing_tolerates_missing_groups() {
        let l: Listing = serde_json::from_str(
            r#"{"dirs":[{"docid":"gns://A/B","name":"x","size":-1,"modified":1}]}"#,
        )
        .unwrap();
        assert_eq!(l.dirs.len(), 1);
        assert!(l.files.is_empty());
    }

    #[test]
    fn api_error_prefers_message_and_cause() {
        let t = r#"{"code":400041003,"cause":"参数不合法","message":"state验证失败"}"#;
        assert_eq!(api_error(t), "state验证失败（参数不合法）");
        assert_eq!(api_error("plain"), "plain");
    }
}
