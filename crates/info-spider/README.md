<div align="center">

# pkuinfo-spider

*A command-line crawler for WeChat Official Account (微信公众号) articles.*

[![crates.io](https://img.shields.io/crates/v/pkuinfo-spider?style=flat-square&logo=rust)](https://crates.io/crates/pkuinfo-spider)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](https://github.com/pkuinfo/pkucli/blob/main/LICENSE)

</div>

Search, list, and scrape articles from WeChat Official Accounts via the MP backend. Convert articles to clean Markdown for archival or further processing.

## Install

```bash
cargo install pkuinfo-spider
```

Or install the [complete PKU toolkit](https://crates.io/crates/pku-cli):

```bash
cargo install pku-cli
```

## Setup

> [!NOTE]
> Unlike other PKU tools, this crawler uses **WeChat QR login**, not IAAA.
> You need a WeChat account with an Official Account platform access (a test
> account works).

```bash
info-spider login                # Scan QR code with WeChat
info-spider status               # Verify session
```

## Usage

### Search Official Accounts

```bash
info-spider search "人民日报"
# Returns: fakeid list for matching accounts
```

Options:
- `-n <count>` — Number of results (1-20, default 5)
- `--format table|json` — Output format

### List Articles

```bash
info-spider articles --name "人民日报"
# Or use fakeid directly (skips one search step):
info-spider articles --fakeid <FAKEID>
```

Options:
- `--begin <offset>` — Pagination start
- `--count <n>` — Articles per page (default 5, max ~20)
- `-l, --limit <n>` — Total articles to fetch across pages
- `--delay-ms <ms>` — Random delay between requests (anti-crawler, default 1500)
- `--format table|json|jsonl`

### Scrape Single Article

```bash
info-spider scrape https://mp.weixin.qq.com/s/xxxxx
info-spider scrape <url> -o article.md
```

Converts the article to clean Markdown, preserving text, links, and image references.

## How It Works

The crawler mimics normal user behavior: login → new article → hyperlink panel → search account → list articles. Configurable random delays between requests help bypass anti-crawl risk controls.

Session data (token, fingerprint, bizuin) is stored in `~/.config/info-spider/`.

## Anti-Crawler Notes

- Use `--delay-ms` to add random jitter between requests (default 1500ms)
- Avoid running long article crawls back-to-back; WeChat may rate-limit or block
- If you hit a block, wait a few hours before retrying

## Links

- **Repository**: [github.com/pkuinfo/pkucli](https://github.com/pkuinfo/pkucli)
- **Full documentation**: See the main [README](https://github.com/pkuinfo/pkucli#readme)
- **Flow notes**: [`docs/wechat-mp-flow.md`](https://github.com/pkuinfo/pkucli/blob/main/docs/wechat-mp-flow.md)
- **Claude Code Skill**: [`pku-info-spider` on Clawhub](https://clawhub.com)
