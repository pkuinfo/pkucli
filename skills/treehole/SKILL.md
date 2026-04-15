---
name: treehole
description: "PKU Treehole (北大树洞) anonymous forum CLI tool built in Rust. Use this skill when working on the treehole crate, debugging treehole commands, adding features to the treehole CLI, understanding the treehole API, or when the user mentions 树洞, treehole, anonymous posts, tree leaves (树叶), or PKU forum. Also use when dealing with IAAA login flow for treehole, JWT callback, SMS verification, or treehole REST API endpoints. **Also use whenever the user asks about their course schedule / 课表 / 这学期上什么课 / weekly schedule / academic calendar / 本周日程 — the treehole API exposes these via `treehole course`, `treehole schedule`, `treehole academic-cal`, `treehole activity-cal`, and they are the canonical source. Do NOT use the `course` (Blackboard) or `elective` crate to answer schedule questions — `course` is for teaching-platform content (assignments/files) and `elective` only sees the major program (misses 辅修/双学位)."
version: 2.0.0
---

# Treehole - 北大树洞 CLI

A CLI client for PKU's anonymous discussion platform (PKU Helper Treehole).

## Architecture

- **Crate location**: `crates/treehole/`
- **Auth flow**: IAAA SSO (`app_id="PKU Helper"`) → JWT callback at `/chapi/cas_iaaa_login` → optional SMS verify
- **API**: JSON REST at `/chapi/api/v3/*` (modern) and `/chapi/api/*` (legacy)
- **API docs**: `docs/treehole-api.md`

## Key Source Files

- `src/main.rs` — Clap CLI definition with all subcommands
- `src/commands.rs` — Command implementations (login, post, search, etc.)
- `src/api.rs` — HTTP API client, request builders, response types
- `src/display.rs` — Terminal output formatting with `colored` crate
- `src/client.rs` — reqwest client builders (`build` for auth, `build_simple` for IAAA)

## CLI Commands

| Command | Alias | Function |
|---------|-------|----------|
| `login` | | IAAA password/QR login → JWT |
| `logout` / `status` | | Session management |
| `list` | `ls` | Browse posts/feed |
| `show` | | View single post with replies |
| `search` | | Full-text search |
| `post` | | Create post (text, tags, images, rewards/树叶) |
| `reply` | | Reply to a post |
| `like` / `tread` | | Vote on posts |
| `star` / `unstar` / `stars` | | Bookmark management |
| `follow` / `unfollow` | | Follow posts |
| `msg` / `read` | | Notifications |
| `me` | | Profile + own posts |
| `score` | | Grade query (with color rendering) |
| `course` | | **Weekly course schedule (canonical source — includes 主修 + 辅修 + 双学位)** |
| `schedule` | | This week's day-by-day schedule |
| `academic-cal` / `activity-cal` | | Academic / activity calendar |
| `otp` | | TOTP 2FA management (bind/set/show/clear) |

## Auto-Login for AI Agents

```bash
# Check session status
info-auth check

# Auto-login (reads credentials from OS keyring, no password needed)
treehole login -p

# If SMS verification is needed (first login or periodic):
PKU_SMS_CODE=123456 treehole login -p
```

Treehole may require SMS verification on first login or periodically (~30 days). When `PKU_SMS_CODE` env var is set, it auto-confirms sending and submits the code without interactive prompts.

## Development Conventions

- All user-facing strings are in **Chinese** (prompts, errors, output)
- Error handling: `anyhow::Result` with `.context("中文描述")`
- HTTP client uses `redirect(Policy::none())` for manual redirect handling
- Session persisted to `~/.config/info/treehole/` via `info_common::session::Store`
- Credentials resolved via `info_common::credential` (keyring → env → interactive)
- Shared auth from `info-common` crate (see info-common skill for details)
