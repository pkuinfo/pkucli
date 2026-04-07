//! 选课网配置文件管理
//!
//! 配置存储在 ~/.config/info/elective/config.toml

use crate::captcha::CaptchaConfig;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 选课网配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ElectiveConfig {
    /// IAAA 用户名（学号）
    #[serde(default)]
    pub username: Option<String>,
    /// 验证码识别后端
    #[serde(default)]
    pub captcha: CaptchaConfig,
    /// 自动选课目标列表
    #[serde(default)]
    pub auto_elect: Vec<AutoElectCourse>,
}

/// 自动选课目标课程
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoElectCourse {
    /// 课程在补退选列表中的页码（0-indexed）
    pub page_id: usize,
    /// 课程名
    pub name: String,
    /// 教师
    pub teacher: String,
    /// 班号
    pub class_id: String,
}

impl std::fmt::Display for AutoElectCourse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} - {} (班号: {}, 页: {})",
            self.name, self.teacher, self.class_id, self.page_id
        )
    }
}

impl ElectiveConfig {
    pub fn load(config_dir: &Path) -> Result<Self> {
        let path = config_dir.join("config.toml");
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("读取配置文件失败: {}", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("解析配置文件失败: {}", path.display()))
    }

    pub fn save(&self, config_dir: &Path) -> Result<()> {
        let path = config_dir.join("config.toml");
        let content = toml::to_string_pretty(self)
            .context("序列化配置失败")?;
        std::fs::write(&path, content)
            .with_context(|| format!("写入配置文件失败: {}", path.display()))?;
        Ok(())
    }
}
