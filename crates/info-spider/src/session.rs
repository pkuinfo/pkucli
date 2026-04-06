use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use cookie_store::CookieStore;
use directories::ProjectDirs;
use rand::Rng;
use reqwest_cookie_store::CookieStoreMutex;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
    sync::Arc,
};

/// 本地持久化的业务会话信息（不含 cookie；cookie 单独存在 cookies.json）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// 登录成功后从 redirect_url 解析出来的 token
    pub token: String,
    /// 浏览器指纹，整个会话期间保持不变
    pub fingerprint: String,
    /// 登录账号 uin
    pub bizuin: Option<String>,
    /// 登录时间
    pub created_at: DateTime<Utc>,
}

impl Session {
    pub fn new(token: String, fingerprint: String, bizuin: Option<String>) -> Self {
        Self {
            token,
            fingerprint,
            bizuin,
            created_at: Utc::now(),
        }
    }
}

/// 本地状态目录（Linux: ~/.config/info-spider/）。
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn new() -> Result<Self> {
        let dirs = ProjectDirs::from("", "", "info-spider")
            .context("无法定位用户配置目录")?;
        let root = dirs.config_dir().to_path_buf();
        fs::create_dir_all(&root)
            .with_context(|| format!("创建配置目录失败: {}", root.display()))?;
        Ok(Self { root })
    }

    pub fn session_path(&self) -> PathBuf {
        self.root.join("session.json")
    }

    pub fn cookies_path(&self) -> PathBuf {
        self.root.join("cookies.json")
    }

    pub fn config_dir(&self) -> &Path {
        &self.root
    }

    /// 加载会话信息；不存在时返回 None。
    pub fn load_session(&self) -> Result<Option<Session>> {
        let path = self.session_path();
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path)
            .with_context(|| format!("读取 session 文件失败: {}", path.display()))?;
        let sess: Session = serde_json::from_slice(&bytes)
            .with_context(|| format!("解析 session 文件失败: {}", path.display()))?;
        Ok(Some(sess))
    }

    pub fn save_session(&self, session: &Session) -> Result<()> {
        let path = self.session_path();
        let data = serde_json::to_vec_pretty(session)?;
        fs::write(&path, data)
            .with_context(|| format!("写入 session 文件失败: {}", path.display()))?;
        Ok(())
    }

    /// 加载或初始化 cookie store。
    pub fn load_cookie_store(&self) -> Result<Arc<CookieStoreMutex>> {
        let path = self.cookies_path();
        let store = if path.exists() {
            let file = fs::File::open(&path)
                .with_context(|| format!("打开 cookie 文件失败: {}", path.display()))?;
            cookie_store::serde::json::load(BufReader::new(file))
                .map_err(|e| anyhow::anyhow!("解析 cookie 文件失败: {e}"))?
        } else {
            CookieStore::default()
        };
        Ok(Arc::new(CookieStoreMutex::new(store)))
    }

    pub fn save_cookie_store(&self, store: &Arc<CookieStoreMutex>) -> Result<()> {
        let path = self.cookies_path();
        let file = fs::File::create(&path)
            .with_context(|| format!("写入 cookie 文件失败: {}", path.display()))?;
        let guard = store
            .lock()
            .map_err(|e| anyhow::anyhow!("锁定 cookie store 失败: {e}"))?;
        let mut writer = BufWriter::new(file);
        cookie_store::serde::json::save(&guard, &mut writer)
            .map_err(|e| anyhow::anyhow!("序列化 cookie 文件失败: {e}"))?;
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        for p in [self.session_path(), self.cookies_path()] {
            if p.exists() {
                fs::remove_file(&p)
                    .with_context(|| format!("删除 {} 失败", p.display()))?;
            }
        }
        Ok(())
    }
}

/// 生成一个 32 位 lowercase hex fingerprint，仿浏览器 canvas 指纹长度。
pub fn generate_fingerprint() -> String {
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| {
            let n: u8 = rng.gen_range(0..16);
            std::char::from_digit(n as u32, 16).unwrap()
        })
        .collect()
}
