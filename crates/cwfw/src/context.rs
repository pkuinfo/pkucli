//! 用户上下文（userContext）管理
//!
//! Wingsoft 平台把会话级状态塞在一个叫做 `userContext` 的大 JSON 对象里，随每次
//! `commonUpdate_*` / `commonQuery_*` 请求一起回传服务端。
//!
//! 这个结构需要从 `loadRolesMenu.action` 返回的 HTML 中提取出来：
//!
//! ```text
//! <input id="userContext" value="[{_@0-USERINFO.USERID_@:_@2200011523_@,...}]" />
//! ```
//!
//! `_@` 是 Wingsoft 对双引号的转义（避免 HTML 属性引号冲突）。还原回 `"` 后就是
//! 常规 JSON。

use crate::client::{CWFW_BASE, CWFW_PROJECT};
use anyhow::{anyhow, Context, Result};
use regex::Regex;
use reqwest::Client;
use serde_json::{Map, Value};

/// 用户上下文 —— 服务端鉴权 / 过滤条件的承载体
#[derive(Debug, Clone)]
pub struct UserContext {
    inner: Map<String, Value>,
}

impl UserContext {
    pub fn new() -> Self {
        Self { inner: Map::new() }
    }

    pub fn from_map(map: Map<String, Value>) -> Self {
        Self { inner: map }
    }

    pub fn set(&mut self, key: String, value: Value) {
        self.inner.insert(key, value);
    }

    #[allow(dead_code)]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.inner.get(key)
    }

    pub fn to_json_string(&self) -> String {
        Value::Object(self.inner.clone()).to_string()
    }
}

impl Default for UserContext {
    fn default() -> Self {
        Self::new()
    }
}

/// 通过 `loadRolesMenu.action` 拉取初始 userContext。
///
/// 需要已建立了 JSESSIONID 的 cookie jar 客户端。
pub async fn fetch_user_context(client: &Client) -> Result<UserContext> {
    let resp = client
        .post(format!("{CWFW_BASE}/WF_CWBS/loadRolesMenu.action"))
        .header("x-requested-with", "XMLHttpRequest")
        .send()
        .await
        .context("loadRolesMenu 请求失败")?;

    let status = resp.status();
    let text = resp.text().await.context("读取 loadRolesMenu 响应失败")?;
    if !status.is_success() {
        return Err(anyhow!("loadRolesMenu HTTP {status}"));
    }

    // 响应 HTML 里有一个 <input id="userContext" value="[{_@key_@:_@value_@,...}]" />
    let re = Regex::new(r#"id="userContext"[^>]*value="(\[\{[^"]*\}\])""#).expect("静态正则");
    let caps = re
        .captures(&text)
        .ok_or_else(|| anyhow!("loadRolesMenu 响应中未找到 userContext"))?;
    let raw = caps[1].to_string();

    // _@ → "
    let restored = raw.replace("_@", "\"");
    // 尾部可能出现 `,}]` 这种 trailing comma，需要清掉
    let cleaned = strip_trailing_commas(&restored);

    let parsed: Vec<Map<String, Value>> = serde_json::from_str(&cleaned)
        .with_context(|| format!("解析 userContext 失败: {cleaned}"))?;
    let first = parsed
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("userContext 数组为空"))?;

    Ok(UserContext::from_map(first))
}

/// 合并 `loadInitFunction.action` 返回的额外变量
///
/// 响应是一个 `[{varname, data, type, func}, ...]` 列表。这里全都以 `0-<UPPERCASE>` 的形式
/// 塞进 userContext（与前端 $.UC.setData 的逻辑一致）。
pub async fn load_init_function(client: &Client, ctx: &mut UserContext) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct Var {
        varname: String,
        #[serde(default)]
        data: serde_json::Value,
    }

    let resp = client
        .post(format!("{CWFW_BASE}/WF_CWBS/loadInitFunction.action"))
        .header("x-requested-with", "XMLHttpRequest")
        .header(
            "content-type",
            "application/x-www-form-urlencoded; charset=UTF-8",
        )
        .body(String::new())
        .send()
        .await
        .context("loadInitFunction 请求失败")?;

    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow!("loadInitFunction HTTP {status}"));
    }

    let vars: Vec<Var> = resp.json().await.context("解析 loadInitFunction 响应失败")?;
    for v in vars {
        ctx.set(format!("0-{}", v.varname.to_uppercase()), v.data);
    }
    Ok(())
}

/// 给 userContext 填入已知的 "个人酬金查询" 静态绑定（菜单节点等）
pub fn seed_reward_query_context(ctx: &mut UserContext, uid: &str) {
    ctx.set(
        "0-USERINFO.USERID".into(),
        Value::String(uid.to_string()),
    );
    ctx.set("0-UNINO".into(), Value::String(uid.to_string()));
    ctx.set("CURRMENUNODEID".into(), Value::Number(220.into()));
    ctx.set(
        format!("{CWFW_PROJECT}$5743-D.FUNCNO"),
        Value::String(crate::api::REWARD_FUNCNO.to_string()),
    );
    ctx.set(
        format!("{CWFW_PROJECT}$5743-D.DBCODE"),
        Value::String(crate::api::REWARD_DBCODE.to_string()),
    );
    ctx.set(
        format!("{CWFW_PROJECT}$5743-D.YEAR_S"),
        Value::String(crate::api::REWARD_DBCODE.to_string()),
    );
}

fn strip_trailing_commas(s: &str) -> String {
    // 简单正则：`,}` → `}`，`,]` → `]`
    let re = Regex::new(r",(\s*[}\]])").expect("静态正则");
    re.replace_all(s, "$1").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_commas() {
        assert_eq!(strip_trailing_commas("{a:1,}"), "{a:1}");
        assert_eq!(strip_trailing_commas("[1,2,]"), "[1,2]");
    }
}
