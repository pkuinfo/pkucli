---
name: campuscard
description: "PKU Campus Card (校园卡) CLI tool built in Rust. Use this skill when working on the campuscard crate, debugging campus card commands, adding features, or when the user mentions 校园卡, campus card, balance, recharge, payment QR, transaction history, monthly spending, or bdcard.pku.edu.cn. Also use when dealing with Synjones platform auth, berserker-auth flow, mobile User-Agent requirements, or HTTP/1.1 constraints."
version: 2.0.0
---

# Campuscard - 北大校园卡 CLI

A CLI client for PKU's campus card system (Synjones platform at bdcard.pku.edu.cn).

## Architecture

- **Crate location**: `crates/campuscard/`
- **Auth flow**: IAAA SSO (`app_id="portal2017"`) → portal → berserker-auth → JWT
- **API**: JSON REST with `synjones-auth` header
- **Constraints**: Requires mobile UA (`PKUANDROID`) and `http1_only()` (server rejects HTTP/2)

## Key Source Files

- `src/main.rs` — Clap CLI with subcommands
- `src/commands.rs` — Command implementations
- `src/api.rs` — API client with synjones-auth header injection
- `src/display.rs` — Terminal output, QR code rendering
- `src/client.rs` — reqwest client with mobile UA and HTTP/1.1 enforcement

## CLI Commands

| Command | Alias | Function |
|---------|-------|----------|
| `login` / `logout` / `status` | | IAAA → portal → berserker → JWT |
| `info` | | Card balance and details |
| `pay` | | Display payment QR code in terminal |
| `recharge` | | Top up card balance |
| `bills` | `ls` | Transaction history (pagination, monthly filter) |
| `stats` | | Monthly spending statistics |
| `otp` | | TOTP 2FA management |

## Auto-Login for AI Agents

```bash
# Check session status
info-auth check

# Auto-login (reads credentials from OS keyring, no password needed)
campuscard login -p
```

## Development Notes

- The auth chain is the most complex: IAAA → portal → berserker-auth → JWT token
- Must use mobile User-Agent (`PKUANDROID`) or requests will be rejected
- Must use `http1_only()` — the Synjones server does not support HTTP/2
- QR code for payment displayed via `info_common::qr` (viuer or system viewer)
- All user-facing strings in **Chinese**
- Error handling: `anyhow::Result` with `.context("中文描述")`
- Session persisted to `~/.config/info/campuscard/`
- Credentials resolved via `info_common::credential` (keyring → env → interactive)
