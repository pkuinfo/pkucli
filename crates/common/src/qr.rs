//! 终端二维码渲染工具

use anyhow::{anyhow, Result};
use std::path::Path;

/// 二维码展示方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QrDisplayMode {
    /// 在终端内渲染（viuer：Sixel / Kitty / Unicode blocks）
    #[default]
    Terminal,
    /// 用系统图片查看器打开原始文件
    Open,
}

/// 在终端展示二维码图片文件
pub fn render_qr_image(path: &Path, mode: QrDisplayMode) -> Result<()> {
    match mode {
        QrDisplayMode::Open => open_with_system_viewer(path),
        QrDisplayMode::Terminal => {
            let img = image::open(path).map_err(|e| anyhow!("打开二维码图片失败: {e}"))?;

            let (cols, _) = terminal_size::terminal_size()
                .map(|(w, h)| (w.0 as u32, h.0 as u32))
                .unwrap_or((80, 24));
            let width = cols.clamp(20, 60);

            let conf = viuer::Config {
                width: Some(width),
                height: None,
                absolute_offset: false,
                ..Default::default()
            };

            viuer::print(&img, &conf).map_err(|e| anyhow!("终端渲染二维码失败: {e}"))?;

            println!(
                "    \x1b[2m（如扫码失败，可用 --open 参数调用系统查看器打开原图: {}）\x1b[0m",
                path.display()
            );
            Ok(())
        }
    }
}

/// 用系统默认图片查看器打开文件
fn open_with_system_viewer(path: &Path) -> Result<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "start"
    } else {
        "xdg-open"
    };

    std::process::Command::new(opener)
        .arg(path)
        .spawn()
        .map_err(|e| anyhow!("打开图片查看器失败: {e}"))?;

    println!("已用系统查看器打开二维码: {}", path.display());
    Ok(())
}

/// 将字符串内容在终端渲染为二维码（用于自己生成的二维码）
pub fn render_qr_string(content: &str) -> Result<()> {
    use qrcode::render::unicode::Dense1x2;

    let code =
        qrcode::QrCode::new(content.as_bytes()).map_err(|e| anyhow!("生成二维码失败: {e}"))?;
    let modules = code.width() as u32;
    let (sx, sy) = pick_module_scale(modules);
    let quiet = should_draw_quiet_zone(modules, sx);
    let rendered = code
        .render::<Dense1x2>()
        .dark_color(Dense1x2::Light)
        .light_color(Dense1x2::Dark)
        .quiet_zone(quiet)
        .module_dimensions(sx, sy)
        .build();
    println!("{rendered}");
    Ok(())
}

fn pick_module_scale(modules_per_side: u32) -> (u32, u32) {
    if let Ok(v) = std::env::var("QR_SCALE") {
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

    let base_w = modules_per_side;
    let base_h = modules_per_side.div_ceil(2);
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

fn should_draw_quiet_zone(modules_per_side: u32, scale: u32) -> bool {
    let (cols, rows) = match terminal_size::terminal_size() {
        Some((w, h)) => (w.0 as u32, h.0 as u32),
        None => return false,
    };
    let w = (modules_per_side + 8) * scale;
    let h = ((modules_per_side + 8).div_ceil(2)) * scale;
    cols >= w + 4 && rows.saturating_sub(10) >= h
}
