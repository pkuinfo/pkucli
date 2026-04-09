---
name: info-auth
description: "PKU unified credential management CLI (统一凭据管理). Use this skill when the user or AI Agent needs to authenticate with PKU services, manage stored credentials, check session status across all services, or when the user mentions 登录, 凭据, 密钥链, keyring, credential, auth, or asks how to let AI tools auto-login. This is the FIRST skill to use before any other PKU service — it handles secure credential storage so that treehole/course/campuscard/elective can auto-login without passwords."
version: 1.0.0
---

# Info-Auth - PKU 统一凭据管理 CLI

Secure credential management for all PKU IAAA-based CLI tools. Allows AI Agents to trigger login flows **without ever seeing passwords**.

## Why This Exists

AI Agents (like OpenClaw/Claude Code) cannot and should not handle user passwords directly. This tool lets users store credentials once in the OS keyring, then all CLI tools auto-authenticate from the keyring.

## Architecture

- **Crate location**: `crates/info-auth/`
- **Backend**: OS keyring via `keyring` crate
  - Linux: D-Bus Secret Service (GNOME Keyring / KDE Wallet)
  - macOS: Apple Keychain
  - Windows: Windows Credential Manager
- **Keyring service name**: `info-pku`

## CLI Commands

| Command | Alias | Function |
|---------|-------|----------|
| `store` | `save` | Interactively input and store credentials to OS keyring |
| `status` | | Show credential storage status (never shows password) |
| `check` | | Show session status for ALL services (treehole/course/campuscard/elective) |
| `clear` | | Remove credentials from OS keyring |

## Credential Resolution Order

When any CLI tool runs `login -p`, credentials are resolved in this order:

1. **OS Keyring** — stored via `info-auth store` (recommended)
2. **Environment variables** — `PKU_USERNAME` + `PKU_PASSWORD` (for CI/automation)
3. **Interactive prompt** — fallback, asks for username/password via stdin

SMS verification codes follow a similar pattern:
1. **Environment variable** — `PKU_SMS_CODE` (Agent can set this after asking user)
2. **Interactive prompt** — fallback

## For AI Agents — Quick Start

### First-time setup (user does this once manually)

```bash
info-auth store
# User enters username + password interactively
# Credentials stored in OS keyring, encrypted
```

### Agent workflow

```bash
# 1. Check which services have valid sessions
info-auth check

# 2. Login to any service (auto-reads credentials from keyring)
treehole login -p
course login -p
campuscard login -p
elective login -p          # may need: --dual major / --dual minor

# 3. If SMS verification is needed (treehole first login):
#    Ask user for the code, then set env var:
PKU_SMS_CODE=123456 treehole login -p

# 4. Use the service
treehole list
course courses --all
campuscard info
elective show
```

### Key rules for Agents

- **NEVER** pass passwords as CLI arguments
- **NEVER** try to read the keyring directly — use `info-auth status` to check
- If `info-auth check` shows "未登录" or "会话已过期", run `<tool> login -p`
- If login fails with "系统密钥链中未存储凭据", ask user to run `info-auth store`

## Development Notes

- All user-facing strings in **Chinese**
- Error handling: `anyhow::Result` with `.context("中文描述")`
- The `store` command requires password confirmation (enter twice)
- `keyring_has_credential()` returns diagnostic info on failure for debugging
