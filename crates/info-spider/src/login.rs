//! 扫码登录流程。严格模仿浏览器真人操作顺序：
//!
//! 1. GET `/`                                         打开首页，建立初始 cookie
//! 2. POST `/cgi-bin/bizlogin?action=startlogin`       启动扫码会话
//! 3. GET  `/cgi-bin/scanloginqrcode?action=getqrcode` 拉取二维码图片
//! 4. 展示二维码（终端渲染 + 落盘备份）
//! 5. 轮询 `/cgi-bin/scanloginqrcode?action=ask`       直到扫码完成
//! 6. POST `/cgi-bin/bizlogin?action=login`            完成登录，解析 redirect_url 中的 token
//! 7. GET  `/cgi-bin/home?t=home/index...`             跟随跳转访问首页，坐实 session

use crate::{
    client::{self, jitter_sleep, xhr_headers, BASE},
    session::{self, Session, Store},
};
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use rand::Rng;
use serde_json::Value;
use std::{fs, path::Path, time::Duration};
use url::Url;

pub async fn run() -> Result<()> {
    let store = Store::new()?;
    // 如果已有有效会话，提示用户是否需要覆盖
    if let Some(old) = store.load_session()? {
        println!(
            "{} 检测到已有登录会话 token={}，created_at={}。继续将覆盖。",
            "[info]".cyan(),
            old.token,
            old.created_at
        );
    }

    // 为本次登录重新生成 fingerprint + 清空旧 cookies（避免老 uin 冲突）
    let fingerprint = session::generate_fingerprint();
    let _ = fs::remove_file(store.cookies_path());
    let cookie_store = store.load_cookie_store()?;
    let client = client::build(cookie_store.clone())?;

    // ---- Step 1: 访问首页 ----
    println!("{} 访问公众号平台首页...", "[1/7]".green());
    let resp = client
        .get(BASE)
        .send()
        .await
        .context("打开首页失败")?
        .error_for_status()?;
    let _ = resp.bytes().await?;
    jitter_sleep(400).await;

    // ---- Step 2: startlogin ----
    println!("{} 启动扫码登录会话...", "[2/7]".green());
    let session_id = format!(
        "177{}{}",
        chrono::Utc::now().timestamp_millis(),
        {
            let mut rng = rand::thread_rng();
            rng.gen_range(10..99)
        }
    );
    let form = [
        ("userlang", "zh_CN"),
        ("redirect_url", ""),
        ("login_type", "3"),
        ("sessionid", session_id.as_str()),
        ("fingerprint", fingerprint.as_str()),
        ("token", ""),
        ("lang", "zh_CN"),
        ("f", "json"),
        ("ajax", "1"),
    ];
    let resp: Value = client
        .post(format!("{BASE}/cgi-bin/bizlogin?action=startlogin"))
        .headers(xhr_headers(BASE))
        .form(&form)
        .send()
        .await
        .context("startlogin 请求失败")?
        .error_for_status()?
        .json()
        .await?;
    ensure_ok(&resp).context("startlogin 返回错误")?;

    // ---- Step 3: 获取二维码 ----
    println!("{} 下载二维码...", "[3/7]".green());
    let qr_url = format!(
        "{BASE}/cgi-bin/scanloginqrcode?action=getqrcode&random={}",
        rand::thread_rng().gen::<u32>()
    );
    let qr_bytes = client
        .get(&qr_url)
        .header("referer", BASE)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;

    // 微信实际返回的是 JPEG（ContentType 也是 image/jpeg），以 .jpg 保存
    let qr_path = store.config_dir().join("login-qrcode.jpg");
    fs::write(&qr_path, &qr_bytes).with_context(|| format!("保存二维码失败: {}", qr_path.display()))?;

    // ---- Step 4: 展示二维码 ----
    println!("{} 请使用微信扫描下方二维码登录：", "[4/7]".green());
    if let Err(e) = render_qr_in_terminal(&qr_path) {
        println!(
            "{} 终端无法渲染二维码（{e}），请手动打开：{}",
            "[warn]".yellow(),
            qr_path.display()
        );
    } else {
        println!("    {} {}", "（二维码已同时保存到）".dimmed(), qr_path.display());
    }
    println!();

    // ---- Step 5: 轮询 ----
    println!("{} 等待扫码确认...", "[5/7]".green());
    let mut last_status = -1i64;
    loop {
        let url = format!(
            "{BASE}/cgi-bin/scanloginqrcode?action=ask&fingerprint={fingerprint}&token=&lang=zh_CN&f=json&ajax=1"
        );
        let r: Value = client
            .get(&url)
            .headers(xhr_headers(BASE))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let status = r.get("status").and_then(|v| v.as_i64()).unwrap_or(-1);
        if status != last_status {
            match status {
                0 => println!("   等待扫描二维码..."),
                4 => println!("   {} 已扫码，请在手机上点击确认登录", "✓".green()),
                1 => {
                    println!("   {} 手机端已确认，完成登录", "✓".green());
                    break;
                }
                5 | 6 => {
                    return Err(anyhow!("二维码已失效（status={status}），请重新运行 login"));
                }
                _ => println!("   状态 status={status}"),
            }
            last_status = status;
        }
        // 1s + 抖动，贴近浏览器
        let base = 1000 + rand::thread_rng().gen_range(0..500);
        tokio::time::sleep(Duration::from_millis(base)).await;
    }

    jitter_sleep(500).await;

    // ---- Step 6: bizlogin action=login ----
    println!("{} 完成登录握手，提取 token...", "[6/7]".green());
    let form_login = [
        ("userlang", "zh_CN"),
        ("redirect_url", ""),
        ("cookie_forbidden", "0"),
        ("cookie_cleaned", "0"),
        ("plugin_used", "0"),
        ("login_type", "3"),
        ("fingerprint", fingerprint.as_str()),
        ("token", ""),
        ("lang", "zh_CN"),
        ("f", "json"),
        ("ajax", "1"),
    ];
    let login_resp: Value = client
        .post(format!("{BASE}/cgi-bin/bizlogin?action=login"))
        .headers(xhr_headers(BASE))
        .form(&form_login)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    ensure_ok(&login_resp).context("bizlogin action=login 失败")?;

    let redirect_url = login_resp
        .get("redirect_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("响应缺少 redirect_url：{login_resp}"))?;
    let token = extract_token(redirect_url)?;

    // 解析 bizuin from cookie store
    let bizuin = extract_cookie(&cookie_store, "bizuin");

    // ---- Step 7: 访问 home 落地 ----
    println!("{} 跟随跳转访问首页...", "[7/7]".green());
    let home = format!("{BASE}{redirect_url}");
    client
        .get(&home)
        .header("referer", BASE)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;

    // ---- 保存会话 ----
    let sess = Session::new(token.clone(), fingerprint, bizuin.clone());
    store.save_session(&sess)?;
    store.save_cookie_store(&cookie_store)?;

    println!();
    println!("{} 登录成功！", "[done]".green().bold());
    println!("  token   = {}", token.bold());
    if let Some(b) = bizuin {
        println!("  bizuin  = {b}");
    }
    println!("  配置目录 = {}", store.config_dir().display());
    Ok(())
}

fn ensure_ok(v: &Value) -> Result<()> {
    let ret = v
        .pointer("/base_resp/ret")
        .and_then(|x| x.as_i64())
        .unwrap_or(-1);
    if ret != 0 {
        let msg = v
            .pointer("/base_resp/err_msg")
            .and_then(|x| x.as_str())
            .unwrap_or("unknown");
        return Err(anyhow!("ret={ret} err_msg={msg}"));
    }
    Ok(())
}

fn extract_token(redirect_url: &str) -> Result<String> {
    // redirect_url 可能是相对路径：/cgi-bin/home?t=home/index&lang=zh_CN&token=xxx
    let full = if redirect_url.starts_with("http") {
        redirect_url.to_string()
    } else {
        format!("{BASE}{redirect_url}")
    };
    let u = Url::parse(&full).context("解析 redirect_url 失败")?;
    for (k, v) in u.query_pairs() {
        if k == "token" {
            return Ok(v.into_owned());
        }
    }
    Err(anyhow!("redirect_url 中未找到 token: {redirect_url}"))
}

fn extract_cookie(
    store: &std::sync::Arc<reqwest_cookie_store::CookieStoreMutex>,
    name: &str,
) -> Option<String> {
    let guard = store.lock().ok()?;
    for c in guard.iter_any() {
        if c.name() == name {
            return Some(c.value().to_string());
        }
    }
    None
}

/// 把 WeChat 返回的 QR PNG **解码回原始数据**，再用 qrcode crate 以终端半块字符
/// 重新像素级渲染。这样每个 QR module 都精确对齐终端字符单元，手机任何距离都能扫。
fn render_qr_in_terminal(path: &Path) -> Result<()> {
    // 1. 加载图片 → 灰度图。用 ImageReader + with_guessed_format 识别真实格式
    //    （微信返回 JPEG 但我们存盘时可能带任何扩展名）
    let luma = image::ImageReader::open(path)
        .with_context(|| format!("打开二维码图片失败: {}", path.display()))?
        .with_guessed_format()
        .with_context(|| format!("探测图片格式失败: {}", path.display()))?
        .decode()
        .with_context(|| format!("解码二维码图片失败: {}", path.display()))?
        .to_luma8();

    // 2. rqrr 识别并解码
    let mut prep = rqrr::PreparedImage::prepare(luma);
    let grids = prep.detect_grids();
    let grid = grids
        .first()
        .ok_or_else(|| anyhow!("未在 PNG 中识别到二维码"))?;
    let (_meta, content) = grid
        .decode()
        .map_err(|e| anyhow!("解码二维码内容失败: {e}"))?;

    // 3. 用 qrcode 重画，Dense1x2 = 每个终端字符承载上下两个 QR 模块（▀/▄/█/空）
    use qrcode::render::unicode::Dense1x2;
    let code = qrcode::QrCode::new(content.as_bytes())
        .map_err(|e| anyhow!("重建二维码失败: {e}"))?;
    let modules = code.width() as u32;
    let (sx, sy) = pick_module_scale(modules);
    // 只有在终端明显有富余时才画 quiet zone；紧凑终端直接省掉，换取 ~20% 空间
    let quiet = should_draw_quiet_zone(modules, sx);
    let rendered = code
        .render::<Dense1x2>()
        // 反色：深色 module 渲染成亮块，浅色 module 渲染成空白
        // 这样在常见深色终端背景下扫描最稳定
        .dark_color(Dense1x2::Light)
        .light_color(Dense1x2::Dark)
        .quiet_zone(quiet)
        .module_dimensions(sx, sy)
        .build();
    println!("{rendered}");
    Ok(())
}

/// 决定 QR 模块缩放倍率。
///
/// 基线：Dense1x2 (1,1) 每个 QR 模块占 1 列 × 0.5 行。对 WeChat ~37 modules 的 QR
/// 而言，基线下大约 37~45 列 × 19~23 行，对绝大多数终端刚好舒适。只有当终端
/// 非常宽裕、QR 会占比 <20% 时才放大一级，避免"QR 太小看不清"。
///
/// 可用环境变量 `INFO_SPIDER_QR_SCALE=1..5` 强制覆盖。
fn pick_module_scale(modules_per_side: u32) -> (u32, u32) {
    if let Ok(v) = std::env::var("INFO_SPIDER_QR_SCALE") {
        if let Ok(n) = v.parse::<u32>() {
            let n = n.clamp(1, 5);
            return (n, n);
        }
    }

    let (cols, rows) = terminal_size::terminal_size()
        .map(|(w, h)| (w.0 as u32, h.0 as u32))
        .unwrap_or((100, 30));
    if modules_per_side == 0 {
        return (1, 1);
    }

    // 基线宽高（无 quiet zone，Dense1x2）
    let base_w = modules_per_side;
    let base_h = modules_per_side.div_ceil(2);

    // 仅在两个维度都还有 3 倍以上富余时放大 2x；都有 5x+ 富余才放 3x
    let ratio_w = cols / base_w.max(1);
    let ratio_h = rows.saturating_sub(8) / base_h.max(1);
    let r = ratio_w.min(ratio_h);
    let s: u32 = match r {
        0..=2 => 1,
        3..=4 => 2,
        _ => 3,
    };
    (s, s)
}

/// 当终端同时在宽/高方向都能额外塞下至少 8 列 / 4 行时，画 quiet zone 边框；
/// 否则省掉 quiet zone，让 QR 在窄终端也不会挤占过多空间。
fn should_draw_quiet_zone(modules_per_side: u32, scale: u32) -> bool {
    let (cols, rows) = match terminal_size::terminal_size() {
        Some((w, h)) => (w.0 as u32, h.0 as u32),
        None => return false,
    };
    let w = (modules_per_side + 8) * scale;
    let h = ((modules_per_side + 8).div_ceil(2)) * scale;
    // 行方向扣掉 10 行提示文字
    cols >= w + 4 && rows.saturating_sub(10) >= h
}

/// 工具命令：显示当前 session 状态
pub fn status() -> Result<()> {
    let store = Store::new()?;
    match store.load_session()? {
        Some(s) => {
            println!("{} 已登录", "●".green());
            println!("  token      = {}", s.token);
            println!("  fingerprint= {}", s.fingerprint);
            if let Some(b) = s.bizuin {
                println!("  bizuin     = {b}");
            }
            println!("  created_at = {}", s.created_at);
            println!("  config dir = {}", store.config_dir().display());
        }
        None => {
            println!("{} 未登录。运行 `info-spider login` 开始。", "○".red());
        }
    }
    Ok(())
}

pub fn logout() -> Result<()> {
    let store = Store::new()?;
    store.clear()?;
    println!("{} 已清除本地会话", "✓".green());
    Ok(())
}
