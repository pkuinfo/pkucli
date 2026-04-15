<div align="center">

# PKU CLI

*Your campus life, from the terminal.*

[![crates.io](https://img.shields.io/crates/v/pku-cli?style=flat-square&logo=rust&label=pku-cli)](https://crates.io/crates/pku-cli)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2021-orange?style=flat-square&logo=rust)](https://www.rust-lang.org)

A unified command-line toolkit for Peking University services — treehole, courses, campus card, elective, credential management, and WeChat article crawler.

[Getting Started](#getting-started) · [Tools](#tools) · [Claude Code Integration](#claude-code-integration) · [Architecture](#architecture)

</div>

---

## Getting Started

### Install

```bash
cargo install pku-cli
```

One command installs the unified `pku` binary with access to all tools:

```bash
pku treehole login -p        # Login to treehole
pku course courses --all     # List all courses
pku card info                # Campus card balance
pku elective show            # View course selections
pku auth store               # Store IAAA credentials
pku spider search "北大"      # Search WeChat articles
```

> [!TIP]
> You can also install tools individually if you only need specific ones:
> ```bash
> cargo install pku-treehole pku-course pku-campuscard pku-elective pku-auth pkuinfo-spider
> ```

### First-time Setup

**1. Store your IAAA credentials once** (encrypted in your OS keychain):

```bash
info-auth store
```

**2. Login to any service** — credentials resolve automatically:

```bash
treehole login -p
course login -p
campuscard login -p
elective login -p
```

> [!NOTE]
> Passwords are **never** exposed in CLI arguments, environment variables, or logs.
> The `info-auth store` command uses interactive input and stores credentials in the
> OS keychain (macOS Keychain / GNOME Keyring / Windows Credential Manager).

## Tools

### Treehole — 北大树洞

Anonymous forum client with full read/write support.

```bash
treehole list                    # Browse latest posts
treehole search "关键词"          # Full-text search
treehole show 12345              # View post + replies
treehole post -t "Hello world"   # Create a post
treehole score                   # View your grades
treehole schedule                # This week's schedule
```

### Course — 教学网 (Blackboard Learn)

Course management, assignments, lecture recordings.

```bash
course courses --all             # List all courses
course assignments               # Pending assignments
course videos                    # Lecture recordings
course submit                    # Submit homework (interactive)
course browse                    # Interactive course browser
```

### Campuscard — 校园卡

Campus card balance, payments, transaction history.

```bash
campuscard info                  # Card info & balance
campuscard pay                   # Show payment QR code
campuscard bills --month 2026-03 # Transaction history
campuscard stats                 # Monthly spending breakdown
campuscard recharge -a 100       # Top up 100 yuan
```

### Elective — 选课网

Course selection with automated retry and CAPTCHA handling.

```bash
elective show                    # Current selections
elective list                    # Available courses
elective set                     # Pick target courses (interactive)
elective launch -t 15            # Auto-select loop, 15s interval
```

### Info Spider — 微信公众号爬虫

WeChat Official Account article crawler (standalone auth, not IAAA).

```bash
info-spider login                # WeChat QR code login
info-spider search "人民日报"     # Search official accounts
info-spider articles --name "X"  # List articles from an account
info-spider scrape <url>         # Convert article to Markdown
```

## Claude Code Integration

PKU CLI is designed to work with [Claude Code](https://claude.ai/code) as an AI-powered campus assistant.

### Install Skills

**Via [Clawhub](https://clawhub.com) (recommended):**

```bash
npm i -g clawhub
clawhub install pku-treehole
clawhub install pku-course
clawhub install pku-campuscard
clawhub install pku-elective
clawhub install pku-info-spider
clawhub install pku-info-common
clawhub install pku-info-auth
```

**Via Claude Code Plugin Marketplace:**

```
/plugin marketplace add pkuinfo/pkucli
/plugin install pku-cli@pku-cli
```

### Agent Prompt

Use this system prompt to enable Claude Code to manage your PKU services autonomously:

```
You have access to PKU CLI tools. Before using any tool, run `info-auth check`
to verify session status. If a session is expired or missing, run
`<tool> login -p` to auto-login using stored credentials.

Available tools:
- treehole: browse/post/search the PKU anonymous forum
- course: manage Blackboard Learn courses, assignments, videos
- campuscard: check balance, pay, view transaction history
- elective: automated course selection with CAPTCHA solving
- info-spider: crawl WeChat official account articles
- info-auth: manage IAAA credentials (store/check/clear)

Credentials are stored in the OS keyring. Never ask the user for passwords.
Use `info-auth store` only when no credentials exist.
```

> [!IMPORTANT]
> Run `info-auth store` interactively **before** giving Claude Code access.
> AI Agents cannot and should not handle password input — they only call
> `<tool> login -p` which reads from the OS keyring automatically.

## Architecture

```
pku-cli                  Unified entry point — `pku` binary
pkuinfo-common (lib)     Shared: IAAA auth, OTP, session, credential resolution
  │
  ├── pku-treehole       Treehole anonymous forum
  ├── pku-course         Blackboard Learn
  ├── pku-campuscard     Campus card (Synjones)
  ├── pku-elective       Course selection
  └── pku-auth           Credential management

pkuinfo-spider           WeChat article crawler (standalone auth)
```

### Auth Flow

All PKU services share a unified IAAA single-sign-on flow:

```
OS Keyring ──→ IAAA SSO ──→ Service callback ──→ Session persisted
                  │
                  ├── treehole:   JWT token
                  ├── course:     Blackboard session
                  ├── campuscard: Synjones JWT
                  └── elective:   Cookie session
```

Credential resolution order:

1. **OS Keyring** — set by `info-auth store`
2. **Environment variables** — `PKU_USERNAME` + `PKU_PASSWORD`
3. **Interactive prompt** — fallback

## Crates

| Crate | Type | Description |
|-------|------|-------------|
| [`pku-cli`](https://crates.io/crates/pku-cli) | meta | One install for all tools |
| [`pkuinfo-common`](https://crates.io/crates/pkuinfo-common) | lib | IAAA auth & shared utilities |
| [`pku-treehole`](https://crates.io/crates/pku-treehole) | bin | Treehole CLI |
| [`pku-course`](https://crates.io/crates/pku-course) | bin | Blackboard Learn CLI |
| [`pku-campuscard`](https://crates.io/crates/pku-campuscard) | bin | Campus card CLI |
| [`pku-elective`](https://crates.io/crates/pku-elective) | bin | Course selection CLI |
| [`pku-auth`](https://crates.io/crates/pku-auth) | bin | Credential management |
| [`pkuinfo-spider`](https://crates.io/crates/pkuinfo-spider) | bin | WeChat article crawler |
