//! 额外 TLS 信任锚
//!
//! PKU 部分服务器（course.pku.edu.cn 等）证书链配置不规范：
//! leaf 证书由 "GlobalSign GCC R6 AlphaSSL CA 2025" 签发，但服务器握手时
//! 只发了 "AlphaSSL CA 2023" 这条**错误**的中间证书，导致链断。
//! 浏览器通过 AIA fetching 自动从 leaf 的 `Authority Information Access`
//! 字段下载正确中间证书并补全；命令行工具（OpenSSL / rustls）默认不做 AIA。
//!
//! 这里把已知的中间证书内嵌为额外信任根，让 reqwest 直接接受这条链。
//! 中间证书本身仍是 GlobalSign 合法签发的 CA（R6 root → AlphaSSL 2025），
//! 只是把它当 trust anchor 使用，安全语义等同于"信任 GlobalSign R6 全家桶"。

use anyhow::{Context, Result};
use reqwest::Certificate;

/// PKU 子域当前在用的 GlobalSign 中间证书（2027-05-21 到期）
const ALPHASSL_CA_2025_PEM: &[u8] =
    include_bytes!("../certs/globalsign_alphassl_ca_2025.pem");

/// 返回所有 PKU 服务可能用到的额外信任根
pub fn extra_root_certs() -> Result<Vec<Certificate>> {
    Ok(vec![
        Certificate::from_pem(ALPHASSL_CA_2025_PEM)
            .context("解析内嵌 AlphaSSL CA 2025 PEM 失败")?,
    ])
}

/// 给 reqwest ClientBuilder 注入所有额外信任根
pub fn apply_extra_roots(mut builder: reqwest::ClientBuilder) -> Result<reqwest::ClientBuilder> {
    for cert in extra_root_certs()? {
        builder = builder.add_root_certificate(cert);
    }
    Ok(builder)
}
