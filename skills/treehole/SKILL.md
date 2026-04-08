---
name: treehole
description: "PKU Treehole (北大树洞) anonymous forum CLI tool built in Rust. Use this skill when working on the treehole crate, debugging treehole commands, adding features to the treehole CLI, understanding the treehole API, or when the user mentions 树洞, treehole, anonymous posts, tree leaves (树叶), or PKU forum. Also use when dealing with IAAA login flow for treehole, JWT callback, SMS verification, or treehole REST API endpoints."
version: 1.0.0
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
| `score` / `course` / `schedule` | | Academic info |
| `otp` | | TOTP 2FA management (bind/set/show/clear) |

## Development Conventions

- All user-facing strings are in **Chinese** (prompts, errors, output)
- Error handling: `anyhow::Result` with `.context("中文描述")`
- HTTP client uses `redirect(Policy::none())` for manual redirect handling
- Session persisted to `~/.config/info/treehole/` via `info_common::session::Store`
- Shared auth from `info-common` crate (see info-common skill for details)
