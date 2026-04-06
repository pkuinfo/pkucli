use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// info-spider —— 微信公众号文章爬虫 CLI
///
/// 工作原理：登录 mp.weixin.qq.com 后台 → 新建文章 → 超链接面板 → 搜索公众号 → 列出文章。
/// 全过程模拟正常用户点击顺序，最大限度贴近真人行为，降低被风控的概率。
#[derive(Debug, Parser)]
#[command(name = "info-spider", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// 扫码登录微信公众号后台，并把会话持久化到本地
    Login,

    /// 退出登录，清除本地会话文件
    Logout,

    /// 查看当前登录会话状态（token、bizuin、创建时间等）
    Status,

    /// 搜索公众号（返回 fakeid 列表，用于后续文章抓取）
    Search {
        /// 公众号名称或微信号
        query: String,

        /// 返回的结果数量（1~20）
        #[arg(short = 'n', long, default_value_t = 5)]
        count: u32,

        /// 输出格式：table / json
        #[arg(long, default_value = "table")]
        format: String,
    },

    /// 列出指定公众号最近发布的文章（自动翻页）
    Articles {
        /// 公众号名称；会自动调用 search 取第一个匹配
        #[arg(long, value_name = "NAME", conflicts_with = "fakeid")]
        name: Option<String>,

        /// 直接指定目标公众号 fakeid（已知时使用，省一次搜索）
        #[arg(long, value_name = "FAKEID")]
        fakeid: Option<String>,

        /// 起始偏移
        #[arg(long, default_value_t = 0)]
        begin: u32,

        /// 单次请求每页数量（后台默认 5，最大建议 20）
        #[arg(long, default_value_t = 5)]
        count: u32,

        /// 总共最多抓取多少篇（默认 = count，仅拉一页）
        #[arg(short, long)]
        limit: Option<u32>,

        /// 每次翻页之间的随机延迟基准毫秒数（会叠加 0~50% 抖动）
        #[arg(long, default_value_t = 1500)]
        delay_ms: u64,

        /// 输出格式：table / json / jsonl
        #[arg(long, default_value = "table")]
        format: String,
    },

    /// 将单篇文章 URL 抓取为 Markdown
    Scrape {
        /// 文章 URL（通常形如 https://mp.weixin.qq.com/s/xxxx）
        url: String,

        /// 输出到文件；省略则写入 stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}
