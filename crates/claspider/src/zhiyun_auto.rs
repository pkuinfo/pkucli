//! 自动获取智云课堂 JWT
//!
//! 复用 `course` (Blackboard) 已建立的会话，通过教学网录播 SSO 链路拿到
//! 智云课堂在 `.pku.edu.cn` 域下设置的 `_token` cookie，再从其内嵌的 Yii
//! 签名结构里提取真正的 JWT，等价于浏览器手动复制 `_token` 的效果。
//!
//! ## SSO 链路
//!
//! 1. `course.pku.edu.cn/.../videoList.action?course_id=<id>` —— 录播列表
//! 2. `course.pku.edu.cn/.../playVideo.action?token=<perVideoJWT>` —— 录播详情
//!    页面 HTML 里嵌一个 iframe，src 指向 `yjapise.pku.edu.cn/casapi/...`
//! 3. GET 该 casapi URL，响应 302 时通过 `Set-Cookie` 在 `.pku.edu.cn` 域
//!    下落地 `_token` 等若干 cookie
//! 4. 从 cookie jar 中取出 `_token`，URL 解码后里面是
//!    `<hash>:2:{i:0;s:6:"_token";i:1;s:N:"<真JWT>";}` 格式

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use colored::Colorize;
use pkuinfo_common::{session::Store, tls};
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest_cookie_store::CookieStoreMutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const COURSE_APP: &str = "course";
const CLASPIDER_APP: &str = "claspider";
const HOMEPAGE: &str =
    "https://course.pku.edu.cn/webapps/portal/execute/tabs/tabAction?tab_tab_group_id=_1_1";
const VIDEO_LIST: &str =
    "https://course.pku.edu.cn/webapps/bb-streammedia-hqy-BBLEARN/videoList.action";
const PLAY_VIDEO: &str =
    "https://course.pku.edu.cn/webapps/bb-streammedia-hqy-BBLEARN/playVideo.action";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ZhiyunCache {
    jwt: String,
    expires_at: DateTime<Utc>,
}

fn cache_path() -> Result<std::path::PathBuf> {
    let store = Store::new(CLASPIDER_APP)?;
    Ok(store.config_dir().join("zhiyun.json"))
}

fn load_cached() -> Option<String> {
    let path = cache_path().ok()?;
    let bytes = std::fs::read(&path).ok()?;
    let cache: ZhiyunCache = serde_json::from_slice(&bytes).ok()?;
    // 留 5 分钟保护边距
    if cache.expires_at > Utc::now() + chrono::Duration::minutes(5) {
        Some(cache.jwt)
    } else {
        None
    }
}

fn save_cached(jwt: &str) -> Result<()> {
    let path = cache_path()?;
    // 服务端 _token Max-Age 为 57600s (16h)，缓存 15h 留余量
    let cache = ZhiyunCache {
        jwt: jwt.to_string(),
        expires_at: Utc::now() + chrono::Duration::hours(15),
    };
    std::fs::write(&path, serde_json::to_vec_pretty(&cache)?)
        .with_context(|| format!("写入智云缓存失败: {}", path.display()))?;
    Ok(())
}

/// 获取智云 JWT。优先用本地缓存，否则触发 SSO。
pub async fn acquire_token(force_refresh: bool) -> Result<String> {
    if !force_refresh {
        if let Some(jwt) = load_cached() {
            eprintln!("{} 复用缓存的智云 JWT", "[*]".green());
            return Ok(jwt);
        }
    }

    let store = Store::new(COURSE_APP)?;
    let session = store
        .load_session()?
        .ok_or_else(|| anyhow!("未找到教学网会话，请先运行 `course login -p`"))?;
    if session.is_expired() {
        eprintln!(
            "{} 教学网 session.json 已过期，仍尝试用现有 cookies；如失败请运行 `course login -p`",
            "[!]".yellow()
        );
    }
    let cookies = store.load_cookie_store()?;
    let client = build_client(cookies.clone())?;

    eprintln!("{} 通过教学网录播 SSO 获取智云 _token...", "[*]".cyan());

    // 1) 从主页解析当前学期 + 历史课程 ID（去重保序）
    let html = client
        .get(HOMEPAGE)
        .send()
        .await
        .context("访问教学网主页失败")?
        .text()
        .await?;
    // 主页课程链接形如 launcher?type=Course&id=PkId{key=_95268_1,...}
    // 课程内部链接形如 ?course_id=_95268_1
    let course_re = Regex::new(r"(?:course_id=|key=)(_\d+_1)").unwrap();
    let mut seen = std::collections::HashSet::new();
    let course_ids: Vec<String> = course_re
        .captures_iter(&html)
        .filter_map(|c| {
            let id = c[1].to_string();
            seen.insert(id.clone()).then_some(id)
        })
        .collect();
    if course_ids.is_empty() {
        bail!("未在教学网主页发现课程，会话可能已失效");
    }

    // 2) 找一门有录播的课
    let play_re = Regex::new(r#"playVideo\.action\?token=([A-Za-z0-9._\-]+)"#).unwrap();
    let mut play_token: Option<(String, String)> = None;
    for cid in &course_ids {
        let url = format!("{VIDEO_LIST}?course_id={cid}&mode=view");
        let body = match client.get(&url).send().await {
            Ok(r) => r.text().await.unwrap_or_default(),
            Err(_) => continue,
        };
        if let Some(cap) = play_re.captures(&body) {
            play_token = Some((cid.clone(), cap[1].to_string()));
            break;
        }
    }
    let (cid, per_video_jwt) = play_token
        .ok_or_else(|| anyhow!("当前账号下无任何录播，无法触发智云 SSO；请先在教学网选一门有录播的课"))?;
    eprintln!("  录播入口：课程 {cid}");

    // 3) 取 playVideo 页面，提取嵌入的 yjapise casapi URL
    let play_url = format!("{PLAY_VIDEO}?token={per_video_jwt}");
    let body = client
        .get(&play_url)
        .send()
        .await
        .context("访问录播详情页失败")?
        .text()
        .await?;
    let casapi_re =
        Regex::new(r#"https://yjapise\.pku\.edu\.cn/casapi/index\.php\?[^"'\s<>]+"#).unwrap();
    let casapi_url = casapi_re
        .find(&body)
        .ok_or_else(|| anyhow!("playVideo 页面未找到 yjapise casapi 跳转 URL"))?
        .as_str()
        .replace("&amp;", "&");

    // 4) 触发 SSO；reqwest 默认跟随 302，cookie jar 落地 `_token`
    client
        .get(&casapi_url)
        .send()
        .await
        .context("调用 yjapise casapi 失败")?;

    // 5) 从 cookie jar 取 _token，解码后挖出嵌套 JWT
    let raw = {
        let guard = cookies.lock().map_err(|e| anyhow!("锁 cookie store 失败: {e}"))?;
        let v = guard
            .iter_any()
            .find(|c| c.name() == "_token")
            .map(|c| c.value().to_string());
        v.ok_or_else(|| anyhow!("SSO 后未在 cookie jar 中捕获 _token"))?
    };
    let jwt = extract_inner_jwt(&raw)?;

    // 持久化（教学网 cookie 也顺带回写一次，casapi 同时刷新了一些 cookie）
    let _ = store.save_cookie_store(&cookies);
    save_cached(&jwt)?;

    eprintln!("{} 智云 JWT 获取成功（缓存 15 小时）", "[+]".green());
    Ok(jwt)
}

fn build_client(cookies: Arc<CookieStoreMutex>) -> Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "user-agent",
        HeaderValue::from_static(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36",
        ),
    );
    headers.insert(
        "accept",
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        ),
    );
    let builder = reqwest::Client::builder()
        .cookie_provider(cookies)
        .default_headers(headers);
    Ok(tls::apply_extra_roots(builder)?.build()?)
}

/// `_token` (URL 解码后) 形如 `<hash>:2:{i:0;s:6:"_token";i:1;s:N:"<JWT>";}`
fn extract_inner_jwt(raw: &str) -> Result<String> {
    let decoded = percent_decode(raw);
    let re = Regex::new(r#"i:1;s:\d+:"([^"]+)""#).unwrap();
    re.captures(&decoded)
        .map(|c| c[1].to_string())
        .ok_or_else(|| anyhow!("_token cookie 格式异常，无法提取内嵌 JWT"))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_nibble(bytes[i + 1]), hex_nibble(bytes[i + 2])) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_yii_signed_token() {
        // 实抓样本（缩短）：%3A=":", %7B="{", %22=", %3B=";", %7D="}"
        let raw = "deadbeef%3A2%3A%7Bi%3A0%3Bs%3A6%3A%22_token%22%3Bi%3A1%3Bs%3A11%3A%22aaa.bbb.ccc%22%3B%7D";
        let jwt = extract_inner_jwt(raw).unwrap();
        assert_eq!(jwt, "aaa.bbb.ccc");
    }
}
