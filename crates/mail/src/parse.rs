//! 纯函数：密码加密、登录跳转解析、js6 响应解析、文件名、HTML 与文本互转、收件人格式化
//!
//! 全部不碰网络，便于单元测试。

use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use rsa::{BigUint, Pkcs1v15Encrypt, RsaPublicKey};
use serde_json::Value;
use std::sync::OnceLock;

// ─── hex ───────────────────────────────────────────────────────

pub fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn hex_decode(s: &str) -> Result<Vec<u8>> {
    let s = s.trim();
    if !s.is_ascii() {
        bail!("不是合法的十六进制：{s}");
    }
    let s = if s.len() % 2 == 1 {
        format!("0{s}")
    } else {
        s.to_string()
    };
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| anyhow!("不是合法的十六进制：{s}"))
        })
        .collect()
}

// ─── 1.2 密码加密 ──────────────────────────────────────────────

/// jsbn `RSAKey.encrypt` 的等价实现：PKCS#1 v1.5 加密 `password#rand`，输出十六进制。
///
/// 结果左侧补零到模长（1024 位密钥 → 256 个十六进制字符）。
pub fn encrypt_password(
    modulus_hex: &str,
    exponent_hex: &str,
    password: &str,
    rand: &str,
) -> Result<String> {
    let n = BigUint::from_bytes_be(&hex_decode(modulus_hex).context("解析 RSA 模数失败")?);
    let e = BigUint::from_bytes_be(&hex_decode(exponent_hex).context("解析 RSA 指数失败")?);
    let key = RsaPublicKey::new(n, e).context("构造 RSA 公钥失败")?;
    let plain = format!("{password}#{rand}");
    let cipher = key
        .encrypt(&mut rand::thread_rng(), Pkcs1v15Encrypt, plain.as_bytes())
        .context("RSA 加密密码失败")?;
    Ok(hex_encode(&cipher))
}

// ─── 1.2 / 1.3 登录跳转 ────────────────────────────────────────

/// 短信二次验证页带回来的信息
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmsInfo {
    /// 短信页的完整 query string，`mobileLogin` 要原样带上
    pub query: String,
    /// 规范账号名（可能和输入的学号不同）
    pub account_name: String,
    /// 规范邮箱地址 `<canon>@stu.pku.edu.cn`
    pub uid: String,
    /// 打码后的手机号
    pub mobile: String,
}

/// 一次登录跳转的含义
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginStep {
    /// 需要短信验证
    Sms(SmsInfo),
    /// 登录成功，拿到 sid 和邮件服务器
    Session {
        sid: String,
        host: String,
        uid: Option<String>,
    },
    /// 中间跳转，继续 GET
    Next(url::Url),
    /// 登录失败
    Failed(String),
}

fn query_param(url: &url::Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
        .filter(|v| !v.is_empty())
}

/// 网易企业邮登录页常见错误码（`errorType`）。
/// 网页端的错误码表没有抓到，这里只列确定会出现的几个，其余原样显示。
fn describe_error(code: &str) -> String {
    let hint = match code {
        "460" | "412" => "用户名或密码错误",
        "420" | "422" => "帐号被锁定或登录太频繁",
        "500" | "503" => "服务器繁忙",
        _ => "登录失败",
    };
    format!("{hint}（错误码 {code}）")
}

/// 解析登录 POST 之后的 302 `Location`
pub fn classify_location(base: &url::Url, location: &str) -> Result<LoginStep> {
    let url = base
        .join(location)
        .with_context(|| format!("无法解析跳转地址：{location}"))?;
    let host = url.host_str().unwrap_or_default().to_string();

    if url.path().contains("/login/sms/") {
        let account_name = query_param(&url, "account_name").unwrap_or_default();
        let uid = query_param(&url, "uid").unwrap_or_default();
        return Ok(LoginStep::Sms(SmsInfo {
            query: url.query().unwrap_or_default().to_string(),
            account_name,
            uid,
            mobile: query_param(&url, "mobile").unwrap_or_default(),
        }));
    }
    if let Some(sid) = query_param(&url, "sid") {
        return Ok(LoginStep::Session {
            sid,
            host,
            uid: query_param(&url, "uid"),
        });
    }
    for key in [
        "errorType",
        "errortype",
        "errorCode",
        "error",
        "err",
        "code",
    ] {
        if let Some(code) = query_param(&url, key) {
            return Ok(LoginStep::Failed(describe_error(&code)));
        }
    }
    if host.ends_with("qiye.163.com") {
        return Ok(LoginStep::Next(url));
    }
    Ok(LoginStep::Failed(format!("登录被重定向到 {url}")))
}

// ─── 2 js6 响应 ────────────────────────────────────────────────

/// js6 调用的失败原因
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Js6Error {
    /// 会话失效（需要重新登录）
    InvalidSession(String),
    /// 其他业务错误
    Failed { code: String, message: String },
}

impl std::fmt::Display for Js6Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Js6Error::InvalidSession(why) => write!(f, "邮箱会话已失效（{why}）"),
            Js6Error::Failed { code, message } if message.is_empty() => {
                write!(f, "邮箱服务器返回错误 {code}")
            }
            Js6Error::Failed { code, message } => {
                write!(f, "邮箱服务器返回错误 {code}：{message}")
            }
        }
    }
}

impl std::error::Error for Js6Error {}

fn is_session_code(code: &str) -> bool {
    let c = code.to_ascii_uppercase();
    c.contains("SESSION") || c == "FA_NO_LOGIN" || c == "FA_NOT_LOGIN" || c == "FA_SECURITY"
}

/// 解析 `/js6/s` 的响应。成功时返回整个 JSON（`var` 之外还有 `total` 等字段）。
pub fn parse_js6(status: u16, body: &str) -> std::result::Result<Value, Js6Error> {
    if (300..400).contains(&status) {
        return Err(Js6Error::InvalidSession(format!("HTTP {status} 跳转")));
    }
    let v: Value = match serde_json::from_str(body.trim()) {
        Ok(v) => v,
        Err(_) if status == 401 || status == 403 => {
            return Err(Js6Error::InvalidSession(format!("HTTP {status}")))
        }
        Err(_) if body.trim_start().starts_with('<') => {
            return Err(Js6Error::InvalidSession("返回了网页而不是 JSON".into()))
        }
        Err(_) => {
            return Err(Js6Error::Failed {
                code: format!("HTTP {status}"),
                message: snippet(body),
            })
        }
    };
    let code = v.get("code").and_then(Value::as_str).unwrap_or_default();
    if code == "S_OK" {
        return Ok(v);
    }
    let message = ["summary", "msg", "message"]
        .iter()
        .find_map(|k| v.get(*k).and_then(Value::as_str))
        .unwrap_or_default()
        .to_string();
    let code = if code.is_empty() {
        format!("HTTP {status}")
    } else {
        code.to_string()
    };
    if is_session_code(&code) {
        return Err(Js6Error::InvalidSession(code));
    }
    Err(Js6Error::Failed { code, message })
}

pub fn snippet(s: &str) -> String {
    let s = s.trim();
    let cut: String = s.chars().take(200).collect();
    if cut.len() < s.len() {
        format!("{cut}…")
    } else {
        cut
    }
}

// ─── Content-Disposition ───────────────────────────────────────

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(b) = u8::from_str_radix(hex, 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 从 `Content-Disposition` 取文件名：优先 RFC 5987 的 `filename*=UTF-8''…`，其次 `filename="…"`
pub fn disposition_filename(header: &str) -> Option<String> {
    let mut plain = None;
    for part in header.split(';') {
        let part = part.trim();
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().trim_matches('"');
        if key == "filename*" {
            // charset'lang'value
            let encoded = value.splitn(3, '\'').nth(2).unwrap_or(value);
            let name = percent_decode(encoded);
            if !name.is_empty() {
                return Some(name);
            }
        } else if key == "filename" && !value.is_empty() {
            plain = Some(if value.contains('%') {
                percent_decode(value)
            } else {
                value.to_string()
            });
        }
    }
    plain
}

/// Windows 不允许的文件名字符换成下划线
pub fn sanitize_filename(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if r#"\/:*?"<>|"#.contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let s = s.trim_end_matches(['.', ' ']).trim().to_string();
    if s.is_empty() {
        "attachment".into()
    } else {
        s
    }
}

// ─── HTML ↔ 文本 ───────────────────────────────────────────────

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("内置正则有误"))
}

fn decode_entities(s: &str) -> String {
    static ENTITY: OnceLock<Regex> = OnceLock::new();
    re(&ENTITY, r"&(#[0-9]+|#[xX][0-9a-fA-F]+|[a-zA-Z]+);")
        .replace_all(s, |c: &regex::Captures| {
            let name = &c[1];
            let ch = if let Some(hex) = name.strip_prefix("#x").or(name.strip_prefix("#X")) {
                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
            } else if let Some(dec) = name.strip_prefix('#') {
                dec.parse().ok().and_then(char::from_u32)
            } else {
                match name {
                    "nbsp" | "ensp" | "emsp" => Some(' '),
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "middot" => Some('·'),
                    "mdash" => Some('—'),
                    "ndash" => Some('–'),
                    "hellip" => Some('…'),
                    "ldquo" => Some('“'),
                    "rdquo" => Some('”'),
                    "lsquo" => Some('‘'),
                    "rsquo" => Some('’'),
                    "copy" => Some('©'),
                    _ => None,
                }
            };
            ch.map_or_else(|| c[0].to_string(), |ch| ch.to_string())
        })
        .into_owned()
}

/// 把 HTML 邮件正文粗略转成纯文本（段落、换行、列表、链接地址保留）
pub fn html_to_text(html: &str) -> String {
    static DROP: OnceLock<Regex> = OnceLock::new();
    static COMMENT: OnceLock<Regex> = OnceLock::new();
    static LINK: OnceLock<Regex> = OnceLock::new();
    static BR: OnceLock<Regex> = OnceLock::new();
    static BLOCK: OnceLock<Regex> = OnceLock::new();
    static LI: OnceLock<Regex> = OnceLock::new();
    static CELL: OnceLock<Regex> = OnceLock::new();
    static TAG: OnceLock<Regex> = OnceLock::new();
    static BLANKS: OnceLock<Regex> = OnceLock::new();

    let s = re(&COMMENT, r"(?s)<!--.*?-->").replace_all(html, "");
    let s = re(
        &DROP,
        r"(?is)<(script|style|head|title)\b[^>]*>.*?</(script|style|head|title)\s*>",
    )
    .replace_all(&s, "");
    // <a href="url">text</a> → text (url)，文字本身就是地址时只留一份
    let s = re(
        &LINK,
        r#"(?is)<a\b[^>]*?href\s*=\s*["']([^"']+)["'][^>]*>(.*?)</a\s*>"#,
    )
    .replace_all(&s, |c: &regex::Captures| {
        let href = &c[1];
        let text = c[2].trim();
        if href.starts_with("mailto:") || text.is_empty() || text.contains(href) {
            text.to_string()
        } else {
            format!("{text} ({href})")
        }
    });
    let s = re(&BR, r"(?i)<br\s*/?>").replace_all(&s, "\n");
    let s = re(&LI, r"(?i)<li\b[^>]*>").replace_all(&s, "\n• ");
    let s = re(&CELL, r"(?i)</t[dh]\s*>").replace_all(&s, "\t");
    let s = re(
        &BLOCK,
        r"(?i)</?(p|div|tr|table|ul|ol|h[1-6]|blockquote|section|article|header|footer|hr)\b[^>]*>",
    )
    .replace_all(&s, "\n");
    let s = re(&TAG, r"(?s)<[^>]+>").replace_all(&s, "");
    let s = decode_entities(&s);
    let lines: Vec<&str> = s.lines().map(str::trim_end).collect();
    let s = lines.join("\n");
    re(&BLANKS, r"\n{3,}")
        .replace_all(&s, "\n\n")
        .trim()
        .to_string()
}

pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// 纯文本正文 → 网页端格式的 HTML（每行一个 `<div>`，空行用 `<br>` 占位）
pub fn text_to_html(text: &str) -> String {
    text.replace("\r\n", "\n")
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                "<div><br></div>".to_string()
            } else {
                format!("<div>{}</div>", escape_html(line))
            }
        })
        .collect()
}

// ─── 收件人 ────────────────────────────────────────────────────

/// 规范化一个收件人：接受 `a@x` 或 `名字 <a@x>`，返回纯地址（js6 compose 只要地址字符串）
pub fn normalize_recipient(raw: &str) -> Result<String> {
    let s = raw.trim();
    let addr = match (s.rfind('<'), s.rfind('>')) {
        (Some(l), Some(r)) if l < r => &s[l + 1..r],
        _ => s,
    }
    .trim();
    let valid = addr.split('@').count() == 2
        && addr.split('@').all(|p| !p.is_empty())
        && addr.rsplit('@').next().is_some_and(|d| d.contains('.'))
        && !addr
            .chars()
            .any(|c| c.is_whitespace() || "<>,;\"".contains(c));
    if !valid {
        bail!("「{raw}」不是有效的邮箱地址");
    }
    Ok(addr.to_string())
}

/// 逗号/分号分隔也接受（`--to a@x,b@y`），去重保序
pub fn normalize_recipients(raw: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for item in raw {
        for piece in item.split([',', ';', '，', '；']) {
            if piece.trim().is_empty() {
                continue;
            }
            let addr = normalize_recipient(piece)?;
            if !out.iter().any(|a| a.eq_ignore_ascii_case(&addr)) {
                out.push(addr);
            }
        }
    }
    Ok(out)
}

/// 邮件里的地址字段可能是字符串，也可能是字符串数组
pub fn addr_list(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::String(s)) if !s.trim().is_empty() => vec![s.trim().to_string()],
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| match x {
                Value::String(s) => Some(s.trim().to_string()),
                Value::Object(o) => o
                    .get("email")
                    .or(o.get("address"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                _ => None,
            })
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// `"张三" <a@x>` → `张三 <a@x>`
pub fn display_addr(s: &str) -> String {
    s.replace('"', "").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::traits::PublicKeyParts;
    use rsa::RsaPrivateKey;

    #[test]
    fn rsa_password_roundtrip() {
        let mut rng = rand::thread_rng();
        let private = RsaPrivateKey::new(&mut rng, 1024).unwrap();
        // 网页端给的模数带前导 00
        let modulus = format!("00{}", hex_encode(&private.n().to_bytes_be()));
        let exponent = hex_encode(&private.e().to_bytes_be());
        assert_eq!(exponent, "010001");
        for _ in 0..4 {
            let hex = encrypt_password(&modulus, &exponent, "p@ss 密码", "DeQfpLIx").unwrap();
            assert_eq!(hex.len(), 256);
            assert!(hex
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
            let plain = private
                .decrypt(Pkcs1v15Encrypt, &hex_decode(&hex).unwrap())
                .unwrap();
            assert_eq!(String::from_utf8(plain).unwrap(), "p@ss 密码#DeQfpLIx");
        }
    }

    #[test]
    fn hex_helpers() {
        assert_eq!(hex_encode(&[0, 15, 255]), "000fff");
        assert_eq!(hex_decode("10001").unwrap(), vec![1, 0, 1]);
        assert!(hex_decode("zz").is_err());
    }

    fn base() -> url::Url {
        url::Url::parse("https://mailh.qiye.163.com/login/domainEntLogin").unwrap()
    }

    #[test]
    fn location_sms() {
        let loc = "https://mailh.qiye.163.com/static/login/sms/index.html?domain=stu.pku.edu.cn&uid=zhangsan%40stu.pku.edu.cn&account_name=zhangsan&mobile=138****0000&all_secure=1";
        match classify_location(&base(), loc).unwrap() {
            LoginStep::Sms(info) => {
                assert_eq!(info.account_name, "zhangsan");
                assert_eq!(info.uid, "zhangsan@stu.pku.edu.cn");
                assert_eq!(info.mobile, "138****0000");
                assert!(info.query.starts_with("domain=stu.pku.edu.cn&uid="));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn location_session() {
        let loc =
            "https://mailshz.qiye.163.com/static/sirius-web/jump/index.html?sid=abc123&from=login";
        assert_eq!(
            classify_location(&base(), loc).unwrap(),
            LoginStep::Session {
                sid: "abc123".into(),
                host: "mailshz.qiye.163.com".into(),
                uid: None,
            }
        );
    }

    #[test]
    fn location_error_and_next() {
        let loc = "https://mail.stu.pku.edu.cn/?errorType=460&uid=x";
        match classify_location(&base(), loc).unwrap() {
            LoginStep::Failed(m) => assert!(m.contains("密码错误") && m.contains("460")),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            classify_location(&base(), "https://mail.stu.pku.edu.cn/").unwrap(),
            LoginStep::Failed(_)
        ));
        assert!(matches!(
            classify_location(&base(), "/entry/redirect?x=1").unwrap(),
            LoginStep::Next(u) if u.host_str() == Some("mailh.qiye.163.com")
        ));
    }

    #[test]
    fn js6_parsing() {
        let ok = parse_js6(200, r#"{"code":"S_OK","var":[1,2],"total":2}"#).unwrap();
        assert_eq!(ok["total"], 2);
        assert_eq!(
            parse_js6(200, r#"{"code":"FA_INVALID_SESSION"}"#),
            Err(Js6Error::InvalidSession("FA_INVALID_SESSION".into()))
        );
        assert!(matches!(
            parse_js6(302, ""),
            Err(Js6Error::InvalidSession(_))
        ));
        assert!(matches!(
            parse_js6(200, "<html>login</html>"),
            Err(Js6Error::InvalidSession(_))
        ));
        let err = parse_js6(200, r#"{"code":"FA_INVALID_PARAMETER","summary":"bad"}"#).unwrap_err();
        assert_eq!(
            err,
            Js6Error::Failed {
                code: "FA_INVALID_PARAMETER".into(),
                message: "bad".into()
            }
        );
        assert!(err.to_string().contains("FA_INVALID_PARAMETER"));
    }

    #[test]
    fn content_disposition() {
        assert_eq!(
            disposition_filename(
                "attachment; filename=\"a.pdf\"; filename*=UTF-8''%E4%BD%9C%E4%B8%9A%201.pdf"
            )
            .as_deref(),
            Some("作业 1.pdf")
        );
        assert_eq!(
            disposition_filename("attachment; filename=\"report.docx\"").as_deref(),
            Some("report.docx")
        );
        assert_eq!(
            disposition_filename("attachment; filename=%E8%AF%BE.txt").as_deref(),
            Some("课.txt")
        );
        assert_eq!(disposition_filename("inline").as_deref(), None);
        assert_eq!(sanitize_filename("a:b?c."), "a_b_c");
        assert_eq!(sanitize_filename(""), "attachment");
    }

    #[test]
    fn html_text_conversion() {
        let html = r#"<html><head><style>p{color:red}</style></head><body>
<p>同学们好&nbsp;：</p><div>第一行<br>第二行</div>
<ul><li>甲</li><li>乙 &amp; 丙</li></ul>
<p>详见 <a href="https://example.com/x">通知</a> &#20013;&#x6587;</p><!-- hidden --></body></html>"#;
        let text = html_to_text(html);
        assert_eq!(
            text,
            "同学们好 ：\n\n第一行\n第二行\n\n• 甲\n• 乙 & 丙\n\n详见 通知 (https://example.com/x) 中文"
        );
        assert_eq!(
            text_to_html("a < b\n\n\"ok\" & go"),
            "<div>a &lt; b</div><div><br></div><div>&quot;ok&quot; &amp; go</div>"
        );
    }

    #[test]
    fn recipients() {
        assert_eq!(
            normalize_recipient(" \"张三\" <zs@stu.pku.edu.cn> ").unwrap(),
            "zs@stu.pku.edu.cn"
        );
        assert!(normalize_recipient("2100012345").is_err());
        assert!(normalize_recipient("a b@x.com").is_err());
        assert!(normalize_recipient("a@localhost").is_err());
        assert_eq!(
            normalize_recipients(&["a@x.cn, b@y.cn".into(), "A@x.cn".into()]).unwrap(),
            vec!["a@x.cn", "b@y.cn"]
        );
        assert_eq!(
            addr_list(Some(&serde_json::json!(["\"李四\" <ls@pku.edu.cn>"]))),
            vec!["\"李四\" <ls@pku.edu.cn>"]
        );
        assert_eq!(
            addr_list(Some(&serde_json::json!("x@y.cn"))),
            vec!["x@y.cn"]
        );
        assert_eq!(
            display_addr("\"李四\" <ls@pku.edu.cn>"),
            "李四 <ls@pku.edu.cn>"
        );
    }
}
