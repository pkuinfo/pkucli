---
name: elective
description: "PKU Course Selection (选课网) CLI tool built in Rust. Use this skill when working on the elective crate, debugging elective commands, adding features, or when the user mentions 选课, elective, course selection, auto-enroll, CAPTCHA solving, dual-degree, or elective.pku.edu.cn. Also use when dealing with CAPTCHA recognition backends (utool/ttshitu/yunma), automated course enrollment loops, or elective SSO callback."
version: 2.0.0
---

# Elective - 北大选课网 CLI

A CLI client for PKU's course selection system with auto-enrollment automation.

## Architecture

- **Crate location**: `crates/elective/`
- **Auth flow**: IAAA SSO (`app_id="elective"`) → elective SSO endpoint callback
- **API**: HTML scraping + CAPTCHA handling (base64 image recognition)
- **Automation**: Polling loop for auto-enrollment with configurable interval

## Key Source Files

- `src/main.rs` — Clap CLI with subcommands
- `src/commands.rs` — Command implementations including auto-enroll loop
- `src/api.rs` — HTML scraping, CAPTCHA image extraction
- `src/display.rs` — Terminal output formatting
- `src/client.rs` — reqwest client builders

## CLI Commands

| Command | Alias | Function |
|---------|-------|----------|
| `login` | | IAAA login (supports `--dual` for dual-degree students) |
| `logout` / `status` | | Session management |
| `show` | | View current course selections |
| `list` | `ls` | Browse available courses for add/drop |
| `set` | | Add a course to auto-enroll target list |
| `unset` | | Remove from auto-enroll targets |
| `config-captcha` | | Configure CAPTCHA solver backend |
| `launch` | | Start auto-enrollment polling loop |
| `otp` | | TOTP 2FA management |

## CAPTCHA Backends

The `config-captcha` command supports multiple recognition backends:
- `manual` — Display CAPTCHA image, user inputs answer
- `utool` — UTool OCR service
- `ttshitu` — TTShiTu recognition API
- `yunma` — Yunma recognition API

## Auto-Login for AI Agents

```bash
# Check session status
info-auth check

# Auto-login (reads credentials from OS keyring, no password needed)
elective login -p                  # single degree
elective login -p --dual major     # dual degree - major
elective login -p --dual minor     # dual degree - minor
```

Note: Dual-degree students MUST specify `--dual major` or `--dual minor`, otherwise login will fail with an error.

## Development Notes

- Auto-enrollment loop: configurable check interval (default 15s), polls for open slots
- Dual-degree students use `--dual` flag at login for separate session
- CAPTCHA images are base64-encoded, decoded and sent to recognition backend
- All user-facing strings in **Chinese**
- Error handling: `anyhow::Result` with `.context("中文描述")`
- Session persisted to `~/.config/info/elective/`
- Credentials resolved via `info_common::credential` (keyring → env → interactive)
