//! 财务综合信息门户请求体加密
//!
//! 该系统把 `$.ajax` 封装成一层自定义编码，对 `commonQuery_` / `common_` /
//! `commonUpdate_` / `loadDefinition` 等所有接口的 key 和 value 都走两层
//! base64 + 5 字节随机盐 + 偶数位反转 + 后缀 `eEc`。
//!
//! 算法参考 `jquery.picList.js` 中的 `picList.encode` / `encode2`：
//!
//! ```text
//! encode2(x) = base64(utf8(x))
//! encode(x) = let b = base64(utf8(x + rand5())) in
//!             for t in 0..floor(len(b)/2) step 2:
//!               swap b[t] and b[len-1-t]
//!             return b + "eEc"
//! ```
//!
//! 完整封装：`encrypt(x) = encode(encode2(x))`。

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand::Rng;

/// 参照 `picList.randomStr(5)` 使用的字符集
const RAND_CHARSET: &[u8] = b"poiuytrewqasdfghjklmnbvcxzQWERTYUIOPLKJHGFDSAZXCVBNM123456789";

fn random_str(len: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| RAND_CHARSET[rng.gen_range(0..RAND_CHARSET.len())] as char)
        .collect()
}

/// `picList.encode2` — 一次 base64(utf8)
pub fn encode2(input: &str) -> String {
    B64.encode(input.as_bytes())
}

/// `picList.encode` — base64(utf8(input + 5 字节随机盐))，然后偶数位反转，末尾加 `eEc`
pub fn encode(input: &str) -> String {
    let salted = format!("{input}{}", random_str(5));
    let b64 = B64.encode(salted.as_bytes());

    let mut chars: Vec<char> = b64.chars().collect();
    let half = chars.len() / 2;
    for t in 0..half {
        if t % 2 == 0 {
            let last = chars.len() - 1 - t;
            chars.swap(t, last);
        }
    }
    let mut out: String = chars.into_iter().collect();
    out.push_str("eEc");
    out
}

/// 对单个字段（key 或 value）做完整封装。
pub fn encrypt(input: &str) -> String {
    encode(&encode2(input))
}

/// 对一组 (key, value) 整体做加密，返回 `application/x-www-form-urlencoded` 风格的 body。
///
/// 注意：服务端会自动识别加密后的 key，直接和 cookie/session 一起做鉴权。
pub fn encrypt_form<I, K, V>(pairs: I) -> String
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    pairs
        .into_iter()
        .map(|(k, v)| {
            let ek = urlencode(&encrypt(k.as_ref()));
            let ev = urlencode(&encrypt(v.as_ref()));
            format!("{ek}={ev}")
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn urlencode(s: &str) -> String {
    // 只对 application/x-www-form-urlencoded 敏感的字符转义
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `picList.encode` 含随机盐，无法逐字节比对。
    /// 这里只验证 encrypt 结果能被对称地解码回原字符串。
    fn decrypt_for_test(s: &str) -> Option<String> {
        // encode 层：去掉 eEc 尾，反转偶数位
        let inner = s.strip_suffix("eEc")?;
        let mut chars: Vec<char> = inner.chars().collect();
        let half = chars.len() / 2;
        for t in 0..half {
            if t % 2 == 0 {
                let last = chars.len() - 1 - t;
                chars.swap(t, last);
            }
        }
        let b64: String = chars.into_iter().collect();
        let layer1 = String::from_utf8(B64.decode(b64.as_bytes()).ok()?).ok()?;
        // 5 字节随机盐
        if layer1.len() < 5 {
            return None;
        }
        let trimmed = &layer1[..layer1.len() - 5];
        // encode2 层：一次 base64
        let layer2 = String::from_utf8(B64.decode(trimmed.as_bytes()).ok()?).ok()?;
        Some(layer2)
    }

    #[test]
    fn roundtrip_ascii() {
        for s in ["_search", "false", "50", "WF_CWBS", "2200011523"] {
            let encrypted = encrypt(s);
            assert_eq!(decrypt_for_test(&encrypted).as_deref(), Some(s));
        }
    }

    #[test]
    fn roundtrip_chinese() {
        for s in ["查询", "个人酬金查询", "卫家燊"] {
            let encrypted = encrypt(s);
            assert_eq!(decrypt_for_test(&encrypted).as_deref(), Some(s));
        }
    }

    #[test]
    fn different_each_call() {
        // 随机盐应该保证每次 encrypt 输出都不同
        assert_ne!(encrypt("hello"), encrypt("hello"));
    }
}
