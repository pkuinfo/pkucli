---
name: info-spider
description: "WeChat Official Account article crawler (微信公众号爬虫) CLI tool built in Rust. Use this skill when working on the info-spider crate, debugging spider commands, adding features, or when the user mentions 微信, WeChat, 公众号, official account, article scraping, MP platform, or info-spider. Also use when dealing with WeChat QR login flow (NOT IAAA), article-to-Markdown conversion, anti-crawler delays, or mp.weixin.qq.com API."
version: 1.0.0
---

# Info-Spider - 微信公众号爬虫 CLI

A CLI crawler for WeChat Official Account (公众号) articles via the MP backend.

## Architecture

- **Crate location**: `crates/info-spider/`
- **Auth flow**: WeChat QR code login (completely separate from IAAA, does NOT use info-common)
- **API**: mp.weixin.qq.com backend API
- **Config**: `~/.config/info-spider/` (separate from info-common Store)
- **Flow docs**: `docs/wechat-mp-flow.md`

## Key Source Files

- `src/main.rs` — Entry point
- `src/cli.rs` — Clap CLI definition
- `src/commands.rs` — Command implementations
- `src/api.rs` — WeChat MP API client
- `src/session.rs` — Own session persistence (token, fingerprint, bizuin)
- `src/client.rs` — reqwest client builders

## CLI Commands

| Command | Function |
|---------|----------|
| `login` | WeChat QR code scan login to mp.weixin.qq.com |
| `logout` / `status` | Session management |
| `search <QUERY>` | Find Official Accounts by name/ID (returns fakeid list) |
| `articles` | Fetch articles from an OA (`--name` or `--fakeid`) |
| `scrape <URL>` | Convert single article URL to Markdown |

## Articles Command Options

- `--begin` — Start offset for pagination
- `--count` — Articles per page
- `--limit` — Maximum total articles to fetch
- `--delay-ms` — Random delay between requests (anti-crawler)
- `--format {table|json|jsonl}` — Output format

## Development Notes

- **Standalone auth**: Uses its own WeChat QR login, NOT the IAAA flow from info-common
- **Own session.rs**: Stores token, fingerprint, bizuin (different from info-common session format)
- Mimics real user behavior with configurable delays to bypass risk controls
- Article scraping extracts content to clean Markdown
- Multiple output formats: table (default), JSON, JSONL
- All user-facing strings in **Chinese**
- Error handling: `anyhow::Result` with `.context("中文描述")`
