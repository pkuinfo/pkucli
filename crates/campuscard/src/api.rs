//! 校园卡 API 封装
//!
//! 所有 API 调用均通过 `synjones-auth: bearer <JWT>` 头认证，
//! 并附加 `synAccessSource=h5` 参数。

use crate::client::{self, CARD_BASE};
use anyhow::{anyhow, Context, Result};
use chrono::NaiveDate;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

// ─── 通用响应 ─────────────────────────────────────────

#[derive(Deserialize)]
struct ApiResp<T> {
    code: i32,
    #[serde(default)]
    success: bool,
    data: Option<T>,
    msg: Option<String>,
    /// 401 错误时返回 message 而非 msg
    message: Option<String>,
}

fn check_resp<T>(resp: ApiResp<T>) -> Result<T> {
    if resp.code == 401 {
        let msg = resp.message.or(resp.msg).unwrap_or_default();
        return Err(anyhow!(
            "登录已失效（{msg}）。请重新运行 `campuscard login`"
        ));
    }
    if !resp.success || resp.code != 200 {
        let msg = resp
            .message
            .or(resp.msg)
            .unwrap_or_else(|| "未知错误".into());
        return Err(anyhow!("API 错误: {msg}"));
    }
    resp.data.ok_or_else(|| anyhow!("API 响应缺少 data 字段"))
}

// ─── 校园卡信息 ──────────────────────────────────────

#[derive(Deserialize)]
pub struct CardQueryData {
    pub card: Vec<CardInfo>,
}

#[derive(Deserialize)]
pub struct CardInfo {
    pub sno: Option<String>,
    pub name: Option<String>,
    pub account: Option<String>,
    pub cardname: Option<String>,
    pub lostflag: i32,
    pub freezeflag: i32,
    pub expdate: Option<String>,
    pub elec_accamt: i64,
    pub accinfo: Option<Vec<AccInfo>>,
}

#[derive(Deserialize)]
pub struct AccInfo {
    pub balance: i64,
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub acc_type: Option<String>,
    pub daycostamt: Option<i64>,
    pub daycostlimit: Option<i64>,
}

// ─── 付款信息 ────────────────────────────────────────

#[derive(Deserialize)]
pub struct PayInfo {
    pub name: String,
    pub elec_accamt: i64,
    pub payacc: Option<String>,
    pub paytype: Option<String>,
}

/// 批量条码响应
#[derive(Deserialize)]
pub struct BatchBarcode {
    pub barcode: Vec<String>,
}

// ─── 卡冲突 ──────────────────────────────────────────

#[derive(Deserialize)]
pub struct ConflictInfo {
    pub message: Option<String>,
    pub status: Option<String>,
}

// ─── 用卡方式 ────────────────────────────────────────

#[derive(Deserialize)]
pub struct UseCardConfig {
    #[serde(rename = "cardType")]
    pub card_type: String,
    #[serde(rename = "cardTypes")]
    pub card_types: Vec<CardTypeOption>,
}

#[derive(Deserialize)]
pub struct CardTypeOption {
    pub name: String,
    pub code: i32,
}

/// 用卡方式代码
pub const CARD_TYPE_DIGITAL: i32 = 1; // 数字卡（付款码用这个）

// ─── 交易记录 ────────────────────────────────────────

#[derive(Deserialize)]
pub struct TurnoverPage {
    pub records: Vec<Turnover>,
    pub total: i64,
    pub current: i64,
    pub pages: i64,
}

#[derive(Deserialize)]
pub struct Turnover {
    pub resume: Option<String>,
    #[serde(rename = "turnoverType")]
    pub turnover_type: Option<String>,
    pub tranamt: i64,
    #[serde(rename = "cardBalance")]
    pub card_balance: i64,
    #[serde(rename = "effectdateStr")]
    pub effectdate_str: Option<String>,
    pub icon: Option<String>,
}

// ─── 统计 ────────────────────────────────────────────

#[derive(Deserialize)]
pub struct TurnoverCount {
    pub income: f64,
    pub expenses: f64,
}

#[derive(Deserialize)]
pub struct TurnoverCategory {
    #[serde(rename = "turnoverType")]
    pub turnover_type: Option<String>,
    pub amount: f64,
}

// ─── API 客户端 ──────────────────────────────────────

async fn parse_response<T: serde::de::DeserializeOwned>(resp: reqwest::Response) -> Result<T> {
    let status = resp.status();
    let body = resp.text().await.context("读取响应失败")?;

    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(anyhow!("登录已失效。请重新运行 `campuscard login`"));
    }

    serde_json::from_str(&body).with_context(|| {
        if status.is_success() {
            format!("响应解析失败: {}", &body[..body.len().min(100)])
        } else {
            format!("请求失败 (HTTP {status})")
        }
    })
}

pub struct CardApi {
    http: reqwest::Client,
}

impl CardApi {
    pub fn new(jwt: &str) -> Result<Self> {
        let http = client::build_api(jwt)?;
        Ok(Self { http })
    }

    /// 查询校园卡信息
    pub async fn query_card(&self) -> Result<CardQueryData> {
        let url = format!("{CARD_BASE}/berserker-app/ykt/tsm/queryCard?synAccessSource=h5");
        let resp = self.http.get(&url).send().await.context("查询校园卡失败")?;
        let api_resp: ApiResp<CardQueryData> = parse_response(resp).await?;
        check_resp(api_resp)
    }

    /// 获取付款码信息
    pub async fn get_pay_info(&self) -> Result<Vec<PayInfo>> {
        let url = format!("{CARD_BASE}/berserker-app/ykt/tsm/codebarPayinfo?synAccessSource=h5");
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("获取付款信息失败")?;
        let api_resp: ApiResp<Vec<PayInfo>> = parse_response(resp).await?;
        check_resp(api_resp)
    }

    /// 获取付款条码（实际展示的 QR 码内容）
    pub async fn get_barcode(
        &self,
        account: &str,
        payacc: &str,
        paytype: &str,
    ) -> Result<BatchBarcode> {
        let url = format!(
            "{CARD_BASE}/berserker-app/ykt/tsm/batchGetBarCodeGet?account={account}&payacc={payacc}&paytype={paytype}&synAccessSource=h5"
        );
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("获取付款条码失败")?;
        let api_resp: ApiResp<BatchBarcode> = parse_response(resp).await?;
        check_resp(api_resp)
    }

    /// 查询当前用卡方式
    pub async fn get_use_card_config(&self) -> Result<UseCardConfig> {
        let url = format!("{CARD_BASE}/berserker-app/useCard/getUseCardConfig?synAccessSource=h5");
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("查询用卡方式失败")?;
        let api_resp: ApiResp<UseCardConfig> = parse_response(resp).await?;
        check_resp(api_resp)
    }

    /// 切换用卡方式
    pub async fn set_use_card_config(&self, card_type: i32) -> Result<()> {
        let url = format!("{CARD_BASE}/berserker-app/useCard/setUseCardConfig");
        let body = serde_json::json!({
            "cardType": card_type,
            "synAccessSource": "h5"
        });
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("切换用卡方式失败")?;
        let api_resp: ApiResp<serde_json::Value> = parse_response(resp).await?;
        if !api_resp.success || api_resp.code != 200 {
            let msg = api_resp.msg.unwrap_or_else(|| "切换失败".into());
            return Err(anyhow!("切换用卡方式失败: {msg}"));
        }
        Ok(())
    }

    /// 检查用卡冲突（NFC vs 数字卡）
    pub async fn get_conflict_info(&self, account: &str) -> Result<ConflictInfo> {
        let url = format!(
            "{CARD_BASE}/berserker-app/cardConflict/getConflictInfo?fromaccount={account}&flag=1&synAccessSource=h5"
        );
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("检查用卡冲突失败")?;
        let api_resp: ApiResp<ConflictInfo> = parse_response(resp).await?;
        check_resp(api_resp)
    }

    /// 查询交易记录（分页）
    pub async fn get_turnovers(
        &self,
        page: i64,
        size: i64,
        type_id: Option<i64>,
        time_from: Option<&NaiveDate>,
        time_to: Option<&NaiveDate>,
    ) -> Result<TurnoverPage> {
        let mut url = format!(
            "{CARD_BASE}/berserker-search/search/personal/turnover?size={size}&current={page}&synAccessSource=h5"
        );
        if let Some(tid) = type_id {
            url.push_str(&format!("&type={tid}"));
        }
        if let Some(from) = time_from {
            url.push_str(&format!("&timeFrom={from}"));
        }
        if let Some(to) = time_to {
            url.push_str(&format!("&timeTo={to}"));
        }

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("查询交易记录失败")?;
        let api_resp: ApiResp<TurnoverPage> = parse_response(resp).await?;
        check_resp(api_resp)
    }

    /// 月度收支统计
    pub async fn get_turnover_count(
        &self,
        time_from: &NaiveDate,
        time_to: &NaiveDate,
    ) -> Result<TurnoverCount> {
        let url = format!(
            "{CARD_BASE}/berserker-search/statistics/turnover/count?timeFrom={time_from}&timeTo={time_to}&synAccessSource=h5"
        );
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("查询收支统计失败")?;
        let api_resp: ApiResp<TurnoverCount> = parse_response(resp).await?;
        check_resp(api_resp)
    }

    /// 日度支出明细（按日汇总）
    pub async fn get_daily_stats(
        &self,
        month: &str, // "2026-04"
        type_id: i64,
    ) -> Result<HashMap<String, f64>> {
        let url = format!(
            "{CARD_BASE}/berserker-search/statistics/turnover/sum/user?dateStr={month}&dateType=month&statisticsDateStr=day&type={type_id}&synAccessSource=h5"
        );
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("查询日度统计失败")?;
        let api_resp: ApiResp<HashMap<String, f64>> = parse_response(resp).await?;
        check_resp(api_resp)
    }

    /// 分类统计
    pub async fn get_category_stats(
        &self,
        time_from: &NaiveDate,
        time_to: &NaiveDate,
        type_id: i64,
    ) -> Result<Vec<TurnoverCategory>> {
        let url = format!(
            "{CARD_BASE}/berserker-search/statistics/turnover?type={type_id}&timeFrom={time_from}&timeTo={time_to}&synAccessSource=h5"
        );
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("查询分类统计失败")?;
        let api_resp: ApiResp<Vec<TurnoverCategory>> = parse_response(resp).await?;
        check_resp(api_resp)
    }

    /// 创建充值订单，返回 orderid
    pub async fn create_recharge_order(
        &self,
        jwt: &str,
        account: &str,
        amount_yuan: i64,
    ) -> Result<String> {
        let url = format!("{CARD_BASE}/charge/order/thirdOrder");

        let mut params: HashMap<String, String> = HashMap::new();
        params.insert("feeitemid".into(), "401".into());
        params.insert("appid".into(), "56321".into());
        params.insert("tranamt".into(), amount_yuan.to_string());
        params.insert("source".into(), "app".into());
        params.insert("synjones-auth".into(), format!("bearer {jwt}"));
        params.insert("yktcard".into(), account.into());
        params.insert("synAccessSource".into(), "h5".into());
        params.insert(
            "abstracts".into(),
            serde_json::json!({"type": "recharge"}).to_string(),
        );

        sign_params(&mut params);

        // 用不跟随重定向的客户端来获取 Location 头中的 orderid
        // 必须带移动端 UA，否则服务器行为不同
        let no_redir = reqwest::Client::builder()
            .http1_only()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(15))
            .build()?;

        let resp = no_redir
            .post(&url)
            .header("user-agent", "PKUANDROID2.2.0_SM-S938B Dalvik/2.1.0 (Linux; U; Android 15; SM-S938B Build/BP1A.250305.020) okhttp/4.12.0")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(serde_urlencoded::to_string(&params).context("序列化充值参数失败")?)
            .send()
            .await
            .context("充值请求失败")?;

        if !resp.status().is_redirection() {
            return Err(anyhow!("创建充值订单失败: HTTP {}", resp.status()));
        }

        let location = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| anyhow!("充值响应缺少重定向"))?;

        // Location 就是完整的收银台 URL（含 orderid 和 token）
        Ok(location.to_string())
    }
}

// ─── 请求签名 ────────────────────────────────────────

/// 对请求参数进行签名
///
/// 算法：
/// 1. 添加 APP_ID, TIMESTAMP, SIGN_TYPE, NONCE
/// 2. 按 key 字典序排序
/// 3. 拼接 key=value&...&SECRET_KEY=<secret>
/// 4. SHA256 后转大写
fn sign_params(params: &mut HashMap<String, String>) {
    const APP_ID: &str = "56321";
    const SECRET: &str = "0osTIhce7uPvDKHz6aa67bhCukaKoYl4";

    params.insert("APP_ID".into(), APP_ID.into());
    params.insert("TIMESTAMP".into(), timestamp_now());
    params.insert("SIGN_TYPE".into(), "SHA256".into());
    params.insert("NONCE".into(), nonce());

    let mut keys: Vec<&String> = params.keys().collect();
    keys.sort();

    let mut sign_str = String::new();
    for key in keys {
        if key == "SIGN" || key == "SECRET_KEY" {
            continue;
        }
        if let Some(val) = params.get(key.as_str()) {
            if !val.is_empty() {
                sign_str.push_str(key);
                sign_str.push('=');
                sign_str.push_str(val);
                sign_str.push('&');
            }
        }
    }
    sign_str.push_str("SECRET_KEY=");
    sign_str.push_str(SECRET);

    let mut hasher = Sha256::new();
    hasher.update(sign_str.as_bytes());
    let hash = hasher.finalize();
    let sign = hex::encode(hash).to_uppercase();

    params.insert("SIGN".into(), sign);
}

fn timestamp_now() -> String {
    chrono::Local::now().format("%Y%m%d%H%M%S").to_string()
}

fn nonce() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..11)
        .map(|_| {
            let idx = rng.gen_range(0..36);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + idx - 10) as char
            }
        })
        .collect()
}

/// Hex encoding (inline, avoids adding hex crate)
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
    }
}
