//! 网费查询 + 充值（its.pku.edu.cn）
//!
//! ## 认证
//!
//! its.pku.edu.cn 用的是"上网账号 + 密码"，**不是** IAAA SSO。
//! 绝大多数学生的上网密码与 IAAA 相同，所以这里直接复用
//! `pkuinfo_common::credential::resolve_credential`。
//!
//! ## 状态查询
//!
//! POST `/cas/ITSweb`，表单字段 `cmd=select&username=&password=&rid=`。
//! 响应为纯文本：
//! - `Err_UID` / `Err_PWD` — 登录失败
//! - 空字符串或其他 — 登录成功；随后 GET `/myConn.jsp` 可拿到
//!   HTML 形式的账单信息（本月使用/余额/在线会话列表）。
//!
//! ## 充值
//!
//! POST `/paybank/user.PayBankOrderPKU`，字段 `uid, password, amount,
//! verifyCode, paytype, operation=pkuConfirm, from=html`。
//! 验证码图片：GET `/paybank/VerifyImage.jsp?<rand>`。

use crate::client::{self, ITS_BASE};
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use pkuinfo_common::captcha::{self, CaptchaConfig};
use pkuinfo_common::credential::{self, Credential};
use reqwest_cookie_store::CookieStoreMutex;
use scraper::{Html, Selector};
use std::sync::Arc;

/// 账户当前状态
#[derive(Debug, Clone, Default)]
pub struct Status {
    pub username: String,
    pub balance: Option<String>,
    pub monthly_usage: Option<String>,
    pub monthly_fee: Option<String>,
    pub ip_sessions: Vec<IpSession>,
    pub raw: String,
}

#[derive(Debug, Clone)]
pub struct IpSession {
    pub ip: String,
    pub kind: String,
    pub login_time: String,
}

fn new_cookie_store() -> Arc<CookieStoreMutex> {
    Arc::new(CookieStoreMutex::new(Default::default()))
}

/// 登录并拉取账户状态
///
/// 分两次请求：
/// 1. POST `/cas/webLogin`，表单 `iprange=yes&rid=&username=&password=`——
///    这是 ITS 完整登录，返回的 HTML 首页里嵌有余额/月使用/月费用。
/// 2. POST `/cas/ITSweb` `cmd=select`，随后 GET `/myConn.jsp`——
///    拿到当前在线 IP 会话列表。
pub async fn query(username: Option<&str>) -> Result<Status> {
    let cred = credential::resolve_credential(username)?;
    let jar = new_cookie_store();
    let client = client::build_with_cookies(jar)?;

    // ── 第一步：/cas/webLogin 拿到带余额的首页 HTML ──
    let home_html = cas_web_login(&client, &cred).await?;

    if std::env::var("PORTAL_DEBUG").is_ok() {
        let path = std::env::temp_dir().join("portal-home.html");
        let _ = std::fs::write(&path, &home_html);
        eprintln!("[debug] cas/webLogin 首页已保存到 {}", path.display());
    }

    let mut status = parse_billing(&cred.username, &home_html);

    // 账户信息：/netportal/itsUtil?operation=info 返回 JSON 或 HTML 段
    if let Ok(resp) = client
        .get(format!("{ITS_BASE}/netportal/itsUtil?operation=info"))
        .header("referer", "https://its.pku.edu.cn/")
        .header("x-requested-with", "XMLHttpRequest")
        .send()
        .await
    {
        if let Ok(body) = resp.text().await {
            if std::env::var("PORTAL_DEBUG").is_ok() {
                let path = std::env::temp_dir().join("portal-itsutil-info.txt");
                let _ = std::fs::write(&path, &body);
                eprintln!("[debug] itsUtil?operation=info 已保存到 {}", path.display());
            }
            apply_itsutil_info(&mut status, &body);
        }
    }

    // ── 第二步：/cas/ITSweb cmd=select + /myConn.jsp 拿会话列表 ──
    let form = [
        ("cmd", "select"),
        ("username", cred.username.as_str()),
        ("password", cred.password.as_str()),
        ("rid", ""),
    ];
    let rtl = client
        .post(format!("{ITS_BASE}/cas/ITSweb"))
        .header("referer", "https://its.pku.edu.cn/")
        .form(&form)
        .send()
        .await
        .context("its /cas/ITSweb 请求失败")?
        .text()
        .await?;
    match rtl.trim() {
        "Err_UID" => return Err(anyhow!("用户名错误 (Err_UID)")),
        "Err_PWD" => return Err(anyhow!("密码错误 (Err_PWD)")),
        _ => {}
    }

    let myconn_html = client
        .get(format!("{ITS_BASE}/myConn.jsp?r=0.42"))
        .header("referer", "https://its.pku.edu.cn/")
        .send()
        .await
        .context("/myConn.jsp 请求失败")?
        .text()
        .await?;

    if std::env::var("PORTAL_DEBUG").is_ok() {
        let path = std::env::temp_dir().join("portal-myconn.html");
        let _ = std::fs::write(&path, &myconn_html);
        eprintln!("[debug] myConn.jsp 已保存到 {}", path.display());
    }

    status.ip_sessions = parse_sessions(&myconn_html);
    Ok(status)
}

async fn cas_web_login(client: &reqwest::Client, cred: &Credential) -> Result<String> {
    let form = [
        ("iprange", "yes"),
        ("rid", ""),
        ("username", cred.username.as_str()),
        ("password", cred.password.as_str()),
    ];
    let resp = client
        .post(format!("{ITS_BASE}/cas/webLogin"))
        .header("referer", "https://its.pku.edu.cn/")
        .form(&form)
        .send()
        .await
        .context("/cas/webLogin 请求失败")?;
    let body = resp.text().await?;

    // 错误路径：服务器返回 `<script>alert("xxx");history.back();</script>`
    if body.contains("alert(") && body.contains("history.back") {
        if let Some(msg) = regex::Regex::new(r#"alert\("([^"]+)"\)"#)
            .ok()
            .and_then(|rx| rx.captures(&body))
            .map(|c| c[1].to_string())
        {
            return Err(anyhow!("ITS 登录失败: {msg}"));
        }
        return Err(anyhow!("ITS 登录失败（未识别的响应）"));
    }
    Ok(body)
}

/// 解析 /cas/webLogin 返回首页中的余额/月使用/月费用
fn parse_billing(username: &str, html: &str) -> Status {
    let mut status = Status {
        username: username.to_string(),
        raw: html.to_string(),
        ..Default::default()
    };

    // 首页登录后会插入形如 `账户余额：12.34 元` 的文字（精确位置随改版浮动）
    let rx_balance = regex::Regex::new(r"(?:账户余额|余\s*额)\s*[:：]?\s*([0-9.]+)\s*元").ok();
    let rx_month_use = regex::Regex::new(
        r"(?:本月使用|已用流量|本月流量)\s*[:：]?\s*([0-9.]+\s*(?:MB|GB|KB|[Mm][Bb]))",
    )
    .ok();
    let rx_month_fee = regex::Regex::new(r"(?:本月费用|月\s*费用)\s*[:：]?\s*([0-9.]+)\s*元").ok();

    if let Some(rx) = rx_balance.as_ref() {
        if let Some(c) = rx.captures(html) {
            status.balance = Some(c[1].to_string());
        }
    }
    if let Some(rx) = rx_month_use.as_ref() {
        if let Some(c) = rx.captures(html) {
            status.monthly_usage = Some(c[1].to_string());
        }
    }
    if let Some(rx) = rx_month_fee.as_ref() {
        if let Some(c) = rx.captures(html) {
            status.monthly_fee = Some(c[1].to_string());
        }
    }
    status
}

/// 把 `/netportal/itsUtil?operation=info` 的 HTML 响应合并到 Status。
///
/// 该页面是一张 `table.itsutil_account`，奇数 `td.td_name` / 偶数 `td.td_value` 配对，
/// 字段名含全角空格（如 `余　　额`）。我们把每个 `名→值` 对都抽出来，按字段中文取值。
fn apply_itsutil_info(status: &mut Status, body: &str) {
    let doc = Html::parse_fragment(body);
    let Ok(cell_sel) = Selector::parse("td.td_name, td.td_value") else {
        return;
    };

    let cells: Vec<(String, bool)> = doc
        .select(&cell_sel)
        .map(|el| {
            let text = el
                .text()
                .collect::<String>()
                .replace(['\u{3000}', '\u{00a0}'], "")
                .trim()
                .trim_end_matches('：')
                .trim_end_matches(':')
                .to_string();
            let is_name = el
                .value()
                .attr("class")
                .map(|c| c.contains("td_name"))
                .unwrap_or(false);
            (text, is_name)
        })
        .collect();

    // 相邻的 (name, value) 对
    let mut pairs = std::collections::BTreeMap::<String, String>::new();
    let mut i = 0;
    while i + 1 < cells.len() {
        if cells[i].1 && !cells[i + 1].1 {
            pairs.insert(cells[i].0.clone(), cells[i + 1].0.clone());
            i += 2;
        } else {
            i += 1;
        }
    }

    // 余额：形如 `10元` / `-0.5元`
    if status.balance.is_none() {
        if let Some(v) = pairs.get("余额") {
            let trimmed = v.trim_end_matches('元').trim().to_string();
            if !trimmed.is_empty() {
                status.balance = Some(trimmed);
            }
        }
    }
    // 本月使用 / 本月费用 字段（若该版本存在）
    if status.monthly_usage.is_none() {
        if let Some(v) = pairs.get("本月使用").or_else(|| pairs.get("本月流量")) {
            status.monthly_usage = Some(v.clone());
        }
    }
    if status.monthly_fee.is_none() {
        if let Some(v) = pairs.get("本月费用").or_else(|| pairs.get("月费用")) {
            status.monthly_fee = Some(v.trim_end_matches('元').trim().to_string());
        }
    }
}

/// 解析 myConn.jsp 中的 `#ipTable tr[id]`（跳过表头）
fn parse_sessions(html: &str) -> Vec<IpSession> {
    let doc = Html::parse_fragment(html);
    let mut out = Vec::new();
    let Ok(sel) = Selector::parse("#ipTable tr[id]") else {
        return out;
    };
    let Ok(td_sel) = Selector::parse("td") else {
        return out;
    };
    for tr in doc.select(&sel) {
        let tds: Vec<String> = tr
            .select(&td_sel)
            .map(|td| td.text().collect::<String>().trim().to_string())
            .collect();
        // 期望列：IP / 终端类型 / 登录时间 / 操作
        if tds.len() >= 3 {
            out.push(IpSession {
                ip: tds[0].clone(),
                kind: tds[1].clone(),
                login_time: tds[2].clone(),
            });
        }
    }
    out
}

pub fn render_status(s: &Status) {
    println!("{} {}", "●".green(), "网费账户状态".bold());
    println!("  账号 = {}", s.username);
    if let Some(b) = &s.balance {
        println!("  余额 = {} 元", b.green().bold());
    }
    if let Some(u) = &s.monthly_usage {
        println!("  本月使用 = {u}");
    }
    if let Some(f) = &s.monthly_fee {
        println!("  本月费用 = {f} 元");
    }
    if !s.ip_sessions.is_empty() {
        println!();
        println!("  在线会话：");
        for sess in &s.ip_sessions {
            println!(
                "    {} [{}] 登录于 {}",
                sess.ip.cyan(),
                sess.kind,
                sess.login_time
            );
        }
    }
    if s.balance.is_none() && s.monthly_usage.is_none() && s.ip_sessions.is_empty() {
        println!();
        println!(
            "  {} 未能解析到余额/会话——myConn.jsp 的结构可能已变更",
            "[warn]".yellow()
        );
        println!("  原始 HTML 前 300 字符：");
        let head: String = s.raw.chars().take(300).collect();
        println!("  {head}");
    }
}

/// 低余额监测：余额低于 `threshold`（单位：元）时返回 true
pub fn is_low(s: &Status, threshold: f64) -> bool {
    s.balance
        .as_deref()
        .and_then(|v| v.parse::<f64>().ok())
        .map(|v| v < threshold)
        .unwrap_or(false)
}

// ─── 充值 ────────────────────────────────────────────────────────

/// 支付方式
#[derive(Debug, Clone, Copy)]
pub enum PayMethod {
    Wechat,
    Alipay,
}

impl PayMethod {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "wechat" | "wx" | "微信" => PayMethod::Wechat,
            "alipay" | "zfb" | "支付宝" => PayMethod::Alipay,
            _ => return Err(anyhow!("未知支付方式: {s}（可选 wechat / alipay）")),
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            PayMethod::Wechat => "微信支付",
            PayMethod::Alipay => "支付宝",
        }
    }
}

/// 充值结果：最终用来生成付款二维码的原始文本
#[derive(Debug, Clone)]
pub struct RechargeResult {
    pub journo: String,
    pub method: PayMethod,
    pub amount: String,
    /// 微信/支付宝二维码的编码文本（扫码即付款）
    pub url_code: String,
}

/// 发起一次充值订单
///
/// 完整流程分 3 步：
/// 1. pkuConfirm — POST `uid,password,amount,verifyCode,operation=pkuConfirm,from=html`，
///    服务器验证凭据+金额+验证码后创建 pending 订单，返回"订单确认"页。
/// 2. pkuSendOrder — POST 上一步隐藏字段 + 新验证码 + `operation=pkuSendOrder`，
///    被重定向到 cwsf 收银台 `showselect`，返回 HTML 含 orderId / orderNo。
/// 3. cashier gotToPay — POST `/PayPreService/pay/cashier/gotToPay`，
///    返回 JSON `{data: {urlCode}}`；urlCode 就是扫码付款内容。
pub async fn recharge(
    username: Option<&str>,
    amount: f64,
    method: PayMethod,
    captcha_cfg: &CaptchaConfig,
    config_dir: &std::path::Path,
) -> Result<RechargeResult> {
    if amount <= 0.0 || amount > 500.0 {
        return Err(anyhow!("充值金额必须在 (0, 500] 元之间"));
    }

    let cred = credential::resolve_credential(username)?;
    let jar = new_cookie_store();
    let client = client::build_with_cookies(jar)?;

    // 访问 epay.jsp 种 JSESSIONID
    client
        .get(format!("{ITS_BASE}/epay.jsp"))
        .send()
        .await
        .context("访问 /epay.jsp 失败")?;

    // ── Step 1: pkuConfirm ──
    let code1 = fetch_captcha(&client, captcha_cfg, config_dir, "step1").await?;
    let form1: Vec<(&'static str, String)> = vec![
        ("uid", cred.username.clone()),
        ("password", cred.password.clone()),
        ("amount", format!("{amount}")),
        ("verifyCode", code1),
        ("paytype", String::new()),
        ("operation", "pkuConfirm".to_string()),
        ("from", "html".to_string()),
    ];
    let (body1, _step1_url) = post_order(&client, &form1, "step1").await?;
    // step1 的成功标志是返回了"订单确认"页（内含 pku_confirm_form）。
    // 如果不是，再尝试从短小的 `<script>alert(...);history.back()</script>` 错误里取 msg。
    let confirm = match parse_confirm_page(&body1) {
        Some(c) => c,
        None => {
            if let Some(msg) = extract_real_alert(&body1) {
                return Err(anyhow!("step1 失败: {msg}"));
            }
            return Err(anyhow!(
                "step1 未返回订单确认页，后端可能已变更；前 300 字符：{}",
                body1.chars().take(300).collect::<String>()
            ));
        }
    };
    println!(
        "{} 订单已创建: journo={} 姓名={} 金额={}元",
        "[+]".cyan(),
        confirm.journo,
        confirm.cn,
        confirm.total_fee
    );

    // ── Step 2: pkuSendOrder（需要新验证码）──
    let code2 = fetch_captcha(&client, captcha_cfg, config_dir, "step2").await?;
    let form2: Vec<(&'static str, String)> = vec![
        ("verifyCode", code2),
        ("total_fee", confirm.total_fee.clone()),
        ("journo", confirm.journo.clone()),
        ("uid", confirm.uid.clone()),
        ("cn", confirm.cn.clone()),
        ("status", "active".to_string()),
        ("from", "pkuPay_pc".to_string()),
        ("operation", "pkuSendOrder".to_string()),
    ];
    let (body2, step2_url) = post_order(&client, &form2, "step2").await?;
    if parse_confirm_page(&body2).is_some() {
        // 再次返回确认页 = 二次验证码被拒
        return Err(anyhow!(
            "step2 失败：服务器返回了新的验证码页，通常意味着上一步验证码被拒"
        ));
    }
    if let Some(msg) = extract_real_alert(&body2) {
        return Err(anyhow!("step2 失败: {msg}"));
    }

    // ── Step 3: 收银台 cashier + AJAX gotToPay 获取 urlCode ──
    let cashier = parse_cashier_page(&body2)
        .ok_or_else(|| anyhow!("step2 未返回财务收银台页面（缺少 orderId/orderNo）"))?;
    let url_code = cashier_go_to_pay(&client, &step2_url, &cashier, method).await?;

    Ok(RechargeResult {
        journo: confirm.journo,
        method,
        amount: confirm.total_fee,
        url_code,
    })
}

#[derive(Debug, Default)]
struct ConfirmFields {
    journo: String,
    total_fee: String,
    uid: String,
    cn: String,
}

fn parse_confirm_page(html: &str) -> Option<ConfirmFields> {
    let doc = Html::parse_fragment(html);
    let sel = Selector::parse(r#"form[name="pku_confirm_form"] input[type="hidden"]"#).ok()?;
    let mut c = ConfirmFields::default();
    for el in doc.select(&sel) {
        let name = el.value().attr("name").unwrap_or_default();
        let value = el.value().attr("value").unwrap_or_default().to_string();
        match name {
            "journo" => c.journo = value,
            "total_fee" => c.total_fee = value,
            "uid" => c.uid = value,
            "cn" => c.cn = value,
            _ => {}
        }
    }
    if c.journo.is_empty() || c.total_fee.is_empty() {
        return None;
    }
    Some(c)
}

async fn fetch_captcha(
    client: &reqwest::Client,
    cfg: &CaptchaConfig,
    config_dir: &std::path::Path,
    tag: &str,
) -> Result<String> {
    println!("{} 获取验证码 ({tag})...", "[+]".cyan());
    let bytes = client
        .get(format!("{ITS_BASE}/paybank/VerifyImage.jsp"))
        .header("referer", "https://its.pku.edu.cn/epay.jsp")
        .send()
        .await
        .context("获取验证码图片失败")?
        .bytes()
        .await?;
    let code = captcha::recognize(client, cfg, &bytes, config_dir).await?;
    println!("{} 验证码 ({tag}): {}", "[+]".cyan(), code);
    Ok(code)
}

async fn post_order(
    client: &reqwest::Client,
    form: &[(&'static str, String)],
    tag: &str,
) -> Result<(String, String)> {
    let resp = client
        .post(format!("{ITS_BASE}/paybank/user.PayBankOrderPKU"))
        .header("referer", "https://its.pku.edu.cn/epay.jsp")
        .form(form)
        .send()
        .await
        .context(format!("充值 {tag} 提交失败"))?;
    let final_url = resp.url().to_string();
    let body = resp.text().await?;
    if std::env::var("PORTAL_DEBUG").is_ok() {
        let path = std::env::temp_dir().join(format!("portal-recharge-{tag}.html"));
        let _ = std::fs::write(&path, &body);
        eprintln!(
            "[debug] recharge {tag} 已保存到 {} (final_url={final_url})",
            path.display()
        );
    }
    Ok((body, final_url))
}

/// 只匹配形如 `<script>alert("...");history.back();</script>` 的**真正的**错误页。
/// JS 函数体里的 alert(...) 会被跳过。
fn extract_real_alert(html: &str) -> Option<String> {
    let rx = regex::Regex::new(
        r#"<script[^>]*>\s*alert\(["']([^"']+)["']\)\s*;\s*history\.back\(\)\s*;?\s*</script>"#,
    )
    .ok()?;
    rx.captures(html).map(|c| c[1].to_string())
}

#[derive(Debug, Default)]
struct CashierFields {
    order_id: String,
    order_no: String,
    project_id: String,
}

fn parse_cashier_page(html: &str) -> Option<CashierFields> {
    let doc = Html::parse_fragment(html);
    let sel = Selector::parse(r#"input[type="hidden"]"#).ok()?;
    let mut c = CashierFields::default();
    for el in doc.select(&sel) {
        let id = el.value().attr("id").unwrap_or_default();
        let val = el.value().attr("value").unwrap_or_default().to_string();
        match id {
            "orderId" => c.order_id = val,
            "orderNo" => c.order_no = val,
            "projectId" => c.project_id = val,
            _ => {}
        }
    }
    if c.order_id.is_empty() || c.order_no.is_empty() {
        return None;
    }
    Some(c)
}

/// 调用收银台 `/PayPreService/pay/cashier/gotToPay`，返回 `urlCode`
async fn cashier_go_to_pay(
    client: &reqwest::Client,
    step2_url: &str,
    cashier: &CashierFields,
    method: PayMethod,
) -> Result<String> {
    // step2 的 host（形如 payment.pku.edu.cn）才是收银台 host
    let base = origin_of(step2_url).unwrap_or_else(|| ITS_BASE.to_string());
    let pay_type = match method {
        PayMethod::Wechat => "02",
        PayMethod::Alipay => "01",
    };
    let form: Vec<(&'static str, String)> = vec![
        ("payType", pay_type.to_string()),
        ("id", cashier.order_id.clone()),
        ("orderTradeNo", cashier.order_no.clone()),
        ("userIp", "127.0.0.1".to_string()),
        ("tradeType", "NATIVE".to_string()),
        ("isOpenBillFlag", "0".to_string()),
        ("invoiceItem", String::new()),
        ("invoiceCategory", String::new()),
        ("custType", String::new()),
        ("custName", String::new()),
        ("custTaxNo", String::new()),
        ("custBank", String::new()),
        ("custBankAccount", String::new()),
        ("custPhone", String::new()),
        ("custAddr", String::new()),
        ("custEmail", String::new()),
        ("remarks", String::new()),
        ("isBillAllowed", "0".to_string()),
        ("projectId", cashier.project_id.clone()),
        ("companyType", String::new()),
    ];

    println!("{} 发起 {}", "[+]".cyan(), method.label());
    let resp = client
        .post(format!("{base}/PayPreService/pay/cashier/gotToPay"))
        .header("referer", step2_url)
        .header("x-requested-with", "XMLHttpRequest")
        .form(&form)
        .send()
        .await
        .context("调用收银台 gotToPay 失败")?;

    let text = resp.text().await?;
    if std::env::var("PORTAL_DEBUG").is_ok() {
        let path = std::env::temp_dir().join("portal-recharge-gototopay.json");
        let _ = std::fs::write(&path, &text);
        eprintln!("[debug] gotToPay 响应已保存到 {}", path.display());
    }

    let v: serde_json::Value =
        serde_json::from_str(&text).context("收银台响应不是 JSON（可能后端已改版）")?;
    let code = v
        .get("messageCode")
        .and_then(|c| c.as_str())
        .unwrap_or_default();
    if code != "0" {
        let msg = v
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("未知错误");
        return Err(anyhow!("收银台返回错误: code={code} msg={msg}"));
    }
    let url_code = v
        .pointer("/data/urlCode")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow!("收银台响应缺少 data.urlCode"))?
        .to_string();
    Ok(url_code)
}

fn origin_of(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    Some(format!("{}://{}", parsed.scheme(), parsed.host_str()?))
}

/// 在终端打印付款二维码
pub fn print_qr_terminal(result: &RechargeResult) -> Result<()> {
    use qrcode::{render::unicode::Dense1x2, QrCode};
    let code = QrCode::new(result.url_code.as_bytes()).context("生成二维码失败")?;
    let art = code
        .render::<Dense1x2>()
        .dark_color(Dense1x2::Light)
        .light_color(Dense1x2::Dark)
        .quiet_zone(true)
        .build();
    println!();
    println!(
        "{} {} 付款二维码（journo={} 金额={}元）",
        "●".green().bold(),
        result.method.label(),
        result.journo,
        result.amount
    );
    println!();
    println!("{art}");
    println!();
    println!("  如二维码显示不全，可手动扫码以下字符串：");
    println!("  {}", result.url_code.cyan());
    Ok(())
}
