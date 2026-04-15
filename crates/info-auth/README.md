<div align="center">

# pku-auth

*Secure credential management for PKU CLI tools. Enables AI Agents to auto-login without ever seeing passwords.*

[![crates.io](https://img.shields.io/crates/v/pku-auth?style=flat-square&logo=rust)](https://crates.io/crates/pku-auth)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](https://github.com/pkuinfo/pkucli/blob/main/LICENSE)

</div>

Store your PKU IAAA credentials once in the OS keychain, then all [PKU CLI tools](https://crates.io/crates/pku-cli) can auto-authenticate without password prompts. Designed to let AI Agents (like Claude Code) manage campus workflows safely.

## Install

```bash
cargo install pku-auth
```

Or install the [complete PKU toolkit](https://crates.io/crates/pku-cli):

```bash
cargo install pku-cli
```

## Why This Exists

AI Agents can't be trusted with raw passwords -- they might log them, send them to APIs, or expose them in tool outputs. This tool solves the problem:

1. **User** runs `info-auth store` once, typing the password interactively
2. **OS keyring** stores the password encrypted (Keychain / GNOME Keyring / Credential Manager)
3. **AI Agent** calls `<tool> login -p` -- the tool reads from the keyring directly
4. **Password** never appears in CLI args, env vars, logs, or agent context

## Usage

### Store Credentials (one-time)

```bash
info-auth store
# Prompts: username (学号/职工号), password, confirm
# Stored encrypted in the OS keychain under service "info-pku"
```

### Check Status

```bash
info-auth status                 # Credential storage status (no password shown)
info-auth check                  # Session status for ALL services
```

Example output of `info-auth check`:

```
各服务会话状态：
  ● 树洞    — 会话有效
  ● 教学网  — 会话已过期，需重新登录
  ○ 校园卡  — 未登录
  ● 选课网  — 会话有效
```

### Clear Credentials

```bash
info-auth clear                  # Remove from keyring
```

## Credential Resolution Order

When you run `<tool> login -p`, credentials are resolved in this order:

1. **OS Keyring** — stored via `info-auth store` (recommended)
2. **Environment variables** — `PKU_USERNAME` + `PKU_PASSWORD` (for CI/automation)
3. **Interactive prompt** — fallback

For SMS verification codes (needed for first-time treehole login):

1. **Environment variable** — `PKU_SMS_CODE`
2. **Interactive prompt** — fallback

## For AI Agents

```
You have access to PKU CLI tools. Workflow:

1. Run `info-auth check` to see session status
2. If a session is expired/missing, run `<tool> login -p` (auto-reads keyring)
3. If login fails with "系统密钥链中未存储凭据", ask the user to run
   `info-auth store` manually

NEVER pass passwords as CLI arguments.
NEVER ask users to type passwords into the agent context.
```

## Platform Support

| Platform | Backend |
|----------|---------|
| Linux | D-Bus Secret Service (GNOME Keyring / KDE Wallet) |
| macOS | Apple Keychain |
| Windows | Windows Credential Manager |

## Links

- **Repository**: [github.com/pkuinfo/pkucli](https://github.com/pkuinfo/pkucli)
- **Full documentation**: See the main [README](https://github.com/pkuinfo/pkucli#readme)
- **Claude Code Skill**: [`pku-info-auth` on Clawhub](https://clawhub.com)
