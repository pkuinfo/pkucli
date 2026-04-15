//! 参与人分组持久化：`~/.config/info/bdkj/groups.json`
//!
//! 为 AI Agent / 日常用户保存固定的参与人集合，避免每次预约都要手输学号姓名。
//! 每个 group 是一个命名的 `(学号, 姓名)` 列表，CLI 通过 `--group <name>` 引用。

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::PathBuf};

use crate::login::APP_NAME;
use pkuinfo_common::session::Store;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GroupsFile {
    /// group 名 → 参与人列表
    pub groups: BTreeMap<String, Vec<GroupMember>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMember {
    pub serial: String,
    pub name: String,
}

fn groups_path() -> Result<PathBuf> {
    let store = Store::new(APP_NAME)?;
    Ok(store.config_dir().join("groups.json"))
}

pub fn load() -> Result<GroupsFile> {
    let path = groups_path()?;
    if !path.exists() {
        return Ok(GroupsFile::default());
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("读取 {} 失败", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("解析 {} 失败", path.display()))
}

pub fn save(file: &GroupsFile) -> Result<()> {
    let path = groups_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(file)?;
    fs::write(&path, json).with_context(|| format!("写入 {} 失败", path.display()))?;
    Ok(())
}

pub fn upsert(name: &str, members: Vec<GroupMember>) -> Result<()> {
    if members.is_empty() {
        return Err(anyhow!("参与人列表不能为空"));
    }
    let mut file = load()?;
    file.groups.insert(name.to_string(), members);
    save(&file)
}

pub fn remove(name: &str) -> Result<bool> {
    let mut file = load()?;
    let removed = file.groups.remove(name).is_some();
    if removed {
        save(&file)?;
    }
    Ok(removed)
}

pub fn get(name: &str) -> Result<Vec<GroupMember>> {
    let file = load()?;
    file.groups
        .get(name)
        .cloned()
        .ok_or_else(|| anyhow!("未找到分组 {name}，先运行 `bdkj group set {name} ...`"))
}

/// 解析 "学号:姓名" 列表为 GroupMember
pub fn parse_members(raw: &[String]) -> Result<Vec<GroupMember>> {
    raw.iter()
        .map(|s| {
            let (serial, name) = s
                .split_once(':')
                .ok_or_else(|| anyhow!("参与人格式应为 学号:姓名 —— 实际: {s}"))?;
            Ok(GroupMember {
                serial: serial.trim().to_string(),
                name: name.trim().to_string(),
            })
        })
        .collect()
}
