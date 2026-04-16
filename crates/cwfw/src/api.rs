//! 财务门户 API 封装
//!
//! ## 工作流
//!
//! 平台为 Wingsoft 的 "网上查询" 系统，所有接口都走 `common*_*.action` + 自定义加密。
//! 业务逻辑分成 3 步：
//!
//! 1. `loadDefinition.action` (winno=5802, type=W)
//!    → 拿到 "个人酬金查询" 窗口的 WG 定义，里面包含 `查询` 按钮绑定的
//!    服务端存储过程哈希 (`` `$<md5>:... ``)。哈希每次新会话都会变，必须动态解析。
//!
//! 2. `commonUpdate_responseButtonEvent.action`
//!    → 传入上一步的哈希 + 筛选值 (d.year / d.month_1 / d.month_2 / d.funcno / d.dbcode)
//!      + 全量 userContext，服务端把筛选条件写进会话级 state。
//!
//! 3. `commonQuery_doQuery.action?funcno=5746&projid=WF_CWBS&needcount=true`
//!    → 取结果。返回体是非严格 JSON（未引用的 key）+ HTML 数字实体编码的中文。

use crate::{
    client::{CWFW_BASE, CWFW_PROJECT},
    context::UserContext,
    encrypt,
};
use anyhow::{anyhow, Context, Result};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// "个人酬金查询" 主窗口号
pub const WIN_REWARD_QUERY: &str = "5802";
/// "个人酬金查询" 表单 funcno
pub const FUNC_REWARD_FORM: &str = "5743";
/// "酬金查询结果" 表格 funcno
pub const FUNC_REWARD_GRID: &str = "5746";

/// 查询 "个人酬金查询" 时 d.funcno 和 d.dbcode 的取值
pub const REWARD_FUNCNO: &str = "9-gz1";
pub const REWARD_DBCODE: &str = "gz1";

/// 所有接口都带这个 X-Requested-With
const XHR: &str = "XMLHttpRequest";

pub struct CwfwApi {
    client: Client,
    ctx: UserContext,
    uid: String,
}

impl CwfwApi {
    pub fn new(client: Client, ctx: UserContext, uid: String) -> Self {
        Self { client, ctx, uid }
    }

    /// 加载某个窗口的定义（W = WinGroup）
    ///
    /// 用法：`load_definition("5802", "W")` 返回 "个人酬金查询" 的完整窗口定义。
    pub async fn load_definition(&self, winno: &str, def_type: &str) -> Result<String> {
        self.post_encrypted(
            "loadDefinition.action",
            &[
                ("projid", CWFW_PROJECT),
                ("winno", winno),
                ("type", def_type),
            ],
        )
        .await
    }

    /// 从 5802 窗口定义中解析 "查询" 按钮的存储过程引用
    ///
    /// 返回形如 `` `$d22613f13d5012bca16310d4bf2f08c3 `` 的哈希前缀（不含参数绑定）。
    pub async fn fetch_reward_query_proc(&self) -> Result<String> {
        let def = self.load_definition(WIN_REWARD_QUERY, "W").await?;

        // procs:["`$<hash>:#5743-d.year#;..."]
        let re = Regex::new(r#"procs:\["(`\$[a-f0-9]{32}):"#).expect("静态正则");
        let caps = re
            .captures(&def)
            .ok_or_else(|| anyhow!("未在 5802 窗口定义中找到查询按钮的 proc 哈希"))?;
        Ok(caps[1].to_string())
    }

    /// 调用 `commonUpdate_responseButtonEvent.action`，把筛选条件写入服务端会话
    ///
    /// 必须先调用 `fetch_reward_query_proc` 拿到本次会话的按钮哈希。
    pub async fn set_reward_filter(
        &self,
        proc_hash: &str,
        year: u32,
        month_from: u32,
        month_to: u32,
    ) -> Result<()> {
        let now = chrono::Local::now();
        let current_year = now.format("%Y").to_string();
        let current_month = now.format("%m").to_string();

        // cmd 是一段 JS 对象字面量（服务端 eval）。data0 列表顺序与按钮定义的绑定顺序一致。
        let cmd = serde_json::json!({
            "check_func": "escape",
            "proc0": proc_hash,
            "data0": [
                {"name": "d.year", "value": year.to_string(), "type": "CHAR"},
                {"name": "d.month_2", "value": format!("{:02}", month_to), "type": "CHAR"},
                {"name": "d.month_1", "value": format!("{:02}", month_from), "type": "CHAR"},
                {"name": "d.funcno", "value": REWARD_FUNCNO, "type": "CHAR"},
                {"name": "d.dbcode", "value": REWARD_DBCODE, "type": "CHAR"},
                {"name": "SHOWPUBRWD", "value": "Y", "type": "char"},
                {"name": "REWARDCOLNUM", "value": "15", "type": "char"},
                {"name": "userinfo.userid", "value": &self.uid, "type": "CHAR"},
                {"name": "currentyear", "value": current_year, "type": "CHAR"},
                {"name": "currentmonth", "value": current_month, "type": "CHAR"},
            ],
            "wfName": "", "wfAct": "", "wfNodeId": "", "wfState": "", "wfUniKey": "",
            "wfVars": "", "wfKeyVal": "", "wfComment": "", "wfCollab": "", "wfPdf": "",
        });

        // 同步更新本地 context 里的 5743-D.* 字段，供 userContext 使用
        let mut ctx = self.ctx.clone();
        ctx.set(
            format!("{CWFW_PROJECT}$5743-D.YEAR"),
            year.to_string().into(),
        );
        ctx.set(
            format!("{CWFW_PROJECT}$5743-D.MONTH_1"),
            format!("{:02}", month_from).into(),
        );
        ctx.set(
            format!("{CWFW_PROJECT}$5743-D.MONTH_2"),
            format!("{:02}", month_to).into(),
        );
        ctx.set(
            format!("{CWFW_PROJECT}$5743-D.FUNCNO"),
            REWARD_FUNCNO.into(),
        );
        ctx.set(
            format!("{CWFW_PROJECT}$5743-D.DBCODE"),
            REWARD_DBCODE.into(),
        );
        ctx.set("0-CURRFUNC".into(), serde_json::json!(5743));
        ctx.set("0-CURRBTN".into(), "查询".into());
        ctx.set("0-CURRFUNCTYPE".into(), "3".into());

        let cmd_str = cmd.to_string();
        let ctx_str = ctx.to_json_string();
        let text = self
            .post_encrypted(
                "commonUpdate_responseButtonEvent.action",
                &[
                    ("cmd", cmd_str.as_str()),
                    ("projid", CWFW_PROJECT),
                    ("userContext", ctx_str.as_str()),
                    ("async", "false"),
                ],
            )
            .await?;
        // 服务端成功返回字符串 "pass"；失败返回 `$$$SYS_ERROR$$$...` 或堆栈
        let trimmed = text.trim();
        if trimmed.eq_ignore_ascii_case("pass") || trimmed.starts_with("[{") {
            Ok(())
        } else if trimmed.contains("SYS_ERROR") {
            Err(anyhow!("服务端拒绝筛选更新: {trimmed}"))
        } else {
            // 有些版本返回空对象 / JSON，都视作成功
            Ok(())
        }
    }

    /// 抓取 `commonQuery_doQuery.action`，取最终的数据行
    ///
    /// 服务端的过滤状态已经被前一步 `set_reward_filter` 写好，这里只需要发最小 userData。
    pub async fn do_query(&self, grid_funcno: &str) -> Result<DoQueryResp> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let user_data = format!(
            r#"{{"0-USERINFO.USERID":"{uid}","0-UNINO":"{uid}"}}"#,
            uid = self.uid
        );
        let nd = now_ms.to_string();

        let text = self
            .post_encrypted(
                "commonQuery_doQuery.action",
                &[
                    // 原本 URL 上的 query params，前端 `$.ajax` 封装会把它们整体加密塞进 body。
                    ("funcno", grid_funcno),
                    ("projid", CWFW_PROJECT),
                    ("needcount", "true"),
                    // 正常 body 字段
                    ("_search", "false"),
                    ("nd", nd.as_str()),
                    ("rows", "500"),
                    ("page", "0"),
                    ("sidx", "to_number(rr.pid) ASC"),
                    ("sord", ""),
                    ("userData", user_data.as_str()),
                    ("itemArr", ""),
                    ("sectionArr", ""),
                ],
            )
            .await?;
        DoQueryResp::parse(&text)
    }

    /// 统一的加密 POST 助手。
    ///
    /// 逻辑来自前端 `$.ajax` 的包装层：
    /// - 总是往 body 里塞一个 `a=<userid>` 字段
    /// - 所有 key/value 都走 `picList.encode(picList.encode2(x))`
    /// - 不支持 URL query string（前端会把 query 移到 body 里，我们直接不传）
    async fn post_encrypted(&self, action: &str, extra_pairs: &[(&str, &str)]) -> Result<String> {
        let url = format!("{CWFW_BASE}/WF_CWBS/{action}");

        // 组装 (key, value) 列表：`a` 放最前（对齐前端行为），然后是调用方传入的字段
        let mut pairs: Vec<(&str, &str)> = Vec::with_capacity(extra_pairs.len() + 1);
        pairs.push(("a", self.uid.as_str()));
        for pair in extra_pairs {
            pairs.push(*pair);
        }

        let body = encrypt::encrypt_form(pairs.iter().copied());

        let resp = self
            .client
            .post(&url)
            .header("x-requested-with", XHR)
            .header(
                "content-type",
                "application/x-www-form-urlencoded; charset=UTF-8",
            )
            .header("accept", "application/json, text/javascript, */*; q=0.01")
            .header("origin", CWFW_BASE)
            .header("referer", format!("{CWFW_BASE}/WF_CWBS/main.jsp"))
            .body(body)
            .send()
            .await
            .with_context(|| format!("{action} 请求失败"))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .with_context(|| format!("读取 {action} 响应失败"))?;

        if !status.is_success() {
            return Err(anyhow!(
                "{action} HTTP {status}: {}",
                text.chars().take(500).collect::<String>()
            ));
        }
        Ok(text)
    }

    /// 高层 API：查询 `[year_from..=year_to]` 月份段的个人酬金记录。
    pub async fn query_reward(
        &self,
        year: u32,
        month_from: u32,
        month_to: u32,
    ) -> Result<DoQueryResp> {
        let proc_hash = self.fetch_reward_query_proc().await?;
        self.set_reward_filter(&proc_hash, year, month_from, month_to)
            .await?;
        self.do_query(FUNC_REWARD_GRID).await
    }
}

/// `commonQuery_doQuery.action` 响应
///
/// 响应是 JS 对象字面量风格（未引用的 key），无法直接 `serde_json::from_str`。
/// 这里用正则把 key 补上双引号后再走 serde。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DoQueryResp {
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub page: u64,
    #[serde(default)]
    pub records: u64,
    #[serde(default)]
    pub rows: Vec<DoQueryRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DoQueryRow {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub cell: Vec<String>,
}

impl DoQueryResp {
    /// 将服务端返回的 JS 对象字面量解析成强类型结构。
    pub fn parse(raw: &str) -> Result<Self> {
        let json = normalize_js_object(raw);
        let mut resp: DoQueryResp =
            serde_json::from_str(&json).with_context(|| format!("解析 doQuery 响应失败: {raw}"))?;
        // 解码 HTML 数字实体
        for row in &mut resp.rows {
            for cell in &mut row.cell {
                *cell = decode_numeric_entities(cell);
            }
        }
        Ok(resp)
    }
}

/// 非严格 JSON → 严格 JSON：给没有引号的对象 key 加上引号。
///
/// Wingsoft 返回的格式像 `{total:1,page:1,rows:[{id:"xxx",cell:[...]}]}`，
/// 除了对象 key 没有双引号外其他都符合 JSON。
fn normalize_js_object(raw: &str) -> String {
    // 匹配 `{` 或 `,` 后紧接的标识符 key，然后冒号
    let re = Regex::new(r#"([{,])\s*([A-Za-z_][A-Za-z0-9_]*)\s*:"#).expect("静态正则");
    re.replace_all(raw, r#"$1"$2":"#).into_owned()
}

/// 解码 `&#20167;` 形式的 HTML 数字字符引用
fn decode_numeric_entities(s: &str) -> String {
    let re = Regex::new(r"&#(\d+);").expect("静态正则");
    re.replace_all(s, |caps: &regex::Captures| {
        caps[1]
            .parse::<u32>()
            .ok()
            .and_then(char::from_u32)
            .map(String::from)
            .unwrap_or_else(|| caps[0].to_string())
    })
    .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_basic() {
        let raw = r#"{total:1,page:1,records:8,rows:[{id:"5746_0",cell:["a","b"]}]}"#;
        let normalized = normalize_js_object(raw);
        assert!(normalized.contains(r#""total":"#));
        assert!(normalized.contains(r#""rows":"#));
        assert!(normalized.contains(r#""id":"5746_0""#));
    }

    #[test]
    fn parse_real_response() {
        // 实际抓到的示例（截取 1 行）
        let raw = r#"{total:1,page:1,records:1,rows:[{id:"5746_0",cell:["&#20167;&#23435;","8200908646","",""]}]}"#;
        let resp = DoQueryResp::parse(raw).expect("parse ok");
        assert_eq!(resp.total, 1);
        assert_eq!(resp.rows.len(), 1);
        // 解码中文
        assert_eq!(resp.rows[0].cell[0], "仇宋");
        assert_eq!(resp.rows[0].cell[1], "8200908646");
    }

    #[test]
    fn numeric_entity_decode() {
        assert_eq!(decode_numeric_entities("&#20892;&#19994;"), "农业");
        assert_eq!(decode_numeric_entities("no entity"), "no entity");
    }
}
