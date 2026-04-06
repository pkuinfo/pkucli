//! 成绩颜色渲染 — 基于 pkuhelper-web-score 的 colorize.ts 移植
//!
//! 根据成绩分数将文本渲染为红(低)→黄(中)→绿(高)的渐变色

use colored::{ColoredString, Colorize};

/// 将原始成绩转换为 GPA (4.0制)
/// 返回 None 表示无法判断（合格制、W、I 等）
pub fn score_to_gpa(score: &str) -> Option<f64> {
    let s = score.trim();

    // Try numeric first
    if let Ok(num) = s.parse::<f64>() {
        return Some(numeric_to_gpa(num));
    }

    // Letter grades
    match s {
        "A+" | "A" => Some(4.0),
        "A-" => Some(3.7),
        "B+" => Some(3.3),
        "B" => Some(3.0),
        "B-" => Some(2.7),
        "C+" => Some(2.3),
        "C" => Some(2.0),
        "C-" => Some(1.5),
        "D+" | "D" | "D-" => Some(1.0),
        "F" => Some(0.0),
        _ => None, // 合格/不合格/W/I/P/NP etc.
    }
}

fn numeric_to_gpa(score: f64) -> f64 {
    if score >= 90.0 {
        4.0
    } else if score >= 85.0 {
        3.7
    } else if score >= 82.0 {
        3.3
    } else if score >= 78.0 {
        3.0
    } else if score >= 75.0 {
        2.7
    } else if score >= 72.0 {
        2.3
    } else if score >= 68.0 {
        2.0
    } else if score >= 64.0 {
        1.5
    } else if score >= 60.0 {
        1.0
    } else {
        0.0
    }
}

/// 判断是否挂科
pub fn is_fail(score: &str) -> bool {
    let s = score.trim();
    if let Ok(num) = s.parse::<f64>() {
        return num < 60.0;
    }
    matches!(s, "F" | "不合格" | "NP")
}

/// HSL to RGB conversion (s and l in 0..1, h in 0..360)
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = h / 60.0;
    let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
    let (r1, g1, b1) = if h_prime < 1.0 {
        (c, x, 0.0)
    } else if h_prime < 2.0 {
        (x, c, 0.0)
    } else if h_prime < 3.0 {
        (0.0, c, x)
    } else if h_prime < 4.0 {
        (0.0, x, c)
    } else if h_prime < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    let m = l - c / 2.0;
    (
        ((r1 + m) * 255.0).round() as u8,
        ((g1 + m) * 255.0).round() as u8,
        ((b1 + m) * 255.0).round() as u8,
    )
}

/// 为课程成绩生成颜色
/// Returns colored string with appropriate color based on score
pub fn colorize_score(score: &str) -> ColoredString {
    let s = score.trim();

    if is_fail(s) {
        return s.red().bold();
    }

    match score_to_gpa(s) {
        Some(gpa) => {
            let prec = ((gpa - 1.0) / 3.0).clamp(0.0, 1.0);
            let hue = 120.0 * prec; // 0=red, 120=green
            let (r, g, b) = hsl_to_rgb(hue, 0.8, 0.45);
            s.truecolor(r, g, b).bold()
        }
        None => {
            // 合格制等 - use dimmed style
            if s == "合格" || s == "P" {
                s.green()
            } else if s == "W" {
                s.dimmed()
            } else {
                s.normal()
            }
        }
    }
}

/// 为学期GPA生成颜色
pub fn colorize_gpa(gpa_str: &str) -> ColoredString {
    if let Ok(gpa) = gpa_str.parse::<f64>() {
        let prec = ((gpa - 1.0) / 3.0).clamp(0.0, 1.0);
        let hue = 120.0 * prec;
        let (r, g, b) = hsl_to_rgb(hue, 0.8, 0.45);
        gpa_str.truecolor(r, g, b).bold()
    } else {
        gpa_str.normal()
    }
}

/// 生成一个彩色的进度条来可视化成绩
/// width: 进度条总字符宽度
pub fn score_bar(score: &str, width: usize) -> String {
    let s = score.trim();

    if is_fail(s) {
        return "░".repeat(width).red().to_string();
    }

    match score_to_gpa(s) {
        Some(gpa) => {
            let prec = ((gpa - 1.0) / 3.0).clamp(0.0, 1.0);
            let filled = (prec * width as f64).round() as usize;
            let hue = 120.0 * prec;
            let (r, g, b) = hsl_to_rgb(hue, 0.8, 0.45);
            let bar_filled = "█".repeat(filled).truecolor(r, g, b);
            let bar_empty = "░".repeat(width.saturating_sub(filled)).dimmed();
            format!("{bar_filled}{bar_empty}")
        }
        None => "░".repeat(width).dimmed().to_string(),
    }
}
