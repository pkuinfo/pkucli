# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
# Build entire workspace
cargo build --release

# Build individual binaries
cargo build --release --bin info-spider
cargo build --release --bin treehole
cargo build --release --bin course
cargo build --release --bin campuscard
cargo build --release --bin elective
cargo build --release --bin info-auth

# Lint (zero warnings required — do NOT use #[allow(dead_code)] etc.)
cargo clippy --workspace

# Lint single crate
cargo clippy --bin treehole

# Run a binary
cargo run --bin treehole -- login
cargo run --bin course -- courses --all
cargo run --bin campuscard -- login
cargo run --bin elective -- login
cargo run --bin info-spider -- search "人民日报"
cargo run --bin info-auth -- store     # interactive credential setup
cargo run --bin info-auth -- check     # check all session status
cargo run --bin info-auth -- status    # check keyring status

# Tests (currently no test files exist)
cargo test --workspace
```

## Architecture

Seven crates in a Cargo workspace. Six are CLI binaries; one is a shared library.

```
info-common (lib)          Shared: IAAA auth, OTP, session/cookie persistence, QR rendering, credential resolution
    ├── iaaa.rs            PKU unified auth (password + QR code login)
    ├── otp.rs             TOTP code generation (RFC 6238, for IAAA 手机令牌)
    ├── session.rs         Session/CookieStore JSON persistence → ~/.config/info/<name>/
    ├── credential.rs      Unified credential resolution: session → keyring → env → interactive
    └── qr.rs              Terminal QR display (viuer) or system viewer

info-auth (bin)            Credential management CLI — store/clear/check IAAA credentials
    Uses: info-common for keyring operations
    Purpose: User runs `info-auth store` once to save credentials to OS keyring;
             AI Agents then call `treehole login -p` etc. without ever seeing passwords

treehole (bin)             PKU Treehole anonymous forum CLI
    Uses: info-common for IAAA → JWT callback → optional SMS verify
    API: JSON REST at /chapi/api/v3/* (modern) and /chapi/api/* (legacy)

course (bin)               PKU Teaching Network (Blackboard Learn) CLI
    Uses: info-common for IAAA → Blackboard SSO callback
    API: HTML scraping with scraper crate (no JSON API), multipart upload

campuscard (bin)           PKU Campus Card CLI (bdcard.pku.edu.cn, Synjones platform)
    Uses: info-common for IAAA → portal → berserker-auth → JWT
    API: JSON REST, requires mobile UA + synjones-auth header, HTTP/1.1 only
    Features: payment QR code, recharge, transaction history, monthly stats

elective (bin)             PKU Course Selection (elective.pku.edu.cn) CLI
    Uses: info-common for IAAA → elective SSO callback
    API: HTML scraping, CAPTCHA handling (base64 image recognition)

info-spider (bin)          WeChat Official Account article crawler
    Standalone: own session.rs, own login flow (WeChat QR, not IAAA)
    Config: ~/.config/info-spider/ (separate from common Store)
```

## Auth Flows

**IAAA (treehole + course + campuscard + elective):** All reuse `info_common::iaaa`. Each crate provides its own `IaaaConfig` with a different `app_id` and `redirect_url`:
- treehole: `app_id="PKU Helper"`, redirect to `/chapi/cas_iaaa_login`
- course: `app_id="blackboard"`, redirect to Blackboard SSO endpoint
- campuscard: `app_id="portal2017"`, redirect to portal → berserker-auth → JWT
- elective: `app_id="elective"`, redirect to elective SSO endpoint

After IAAA returns a token, each crate has its own `complete_*_login()` that exchanges the token with the target service and saves session+cookies.

**Credential Resolution Order** (in `info_common::credential`):
1. OS Keyring (`info-pku` service) — set by `info-auth store`
2. Environment variables (`PKU_USERNAME` + `PKU_PASSWORD`)
3. Interactive prompt (fallback)

**AI Agent Safety**: Passwords NEVER appear in CLI arguments. `info-auth store` handles all password input interactively. AI Agents should only call `<tool> login -p` which auto-resolves credentials from keyring/env. Use `info-auth check` to verify session status.

**info-spider:** Completely separate WeChat QR login flow. Does not use `info-common`.

## Session & Config Storage

Treehole/course/campuscard/elective use `info_common::session::Store::new(APP_NAME)` → `~/.config/info/<name>/`:
- `session.json` — token, expires_at, uid, created_at, extra (serde_json::Value)
- `cookies.json` — reqwest CookieStore serialized

info-spider uses its own Store → `~/.config/info-spider/`:
- `session.json` — token, fingerprint, bizuin, created_at
- `cookies.json` — same cookie format

## HTTP Client Pattern

Every crate has a `client.rs` with two builders:
- `build(cookie_store: Arc<CookieStoreMutex>)` — for authenticated requests, persistent cookies
- `build_simple()` — for IAAA login only (internal cookie jar, JSESSIONID handling)

Both set a realistic User-Agent. Treehole uses `redirect(Policy::none())` for manual redirect handling. Campuscard uses mobile UA (`PKUANDROID`) and requires `http1_only()` (server doesn't support HTTP/2).

## Key Conventions

- **Language:** Chinese UI strings throughout (prompts, error messages, display output)
- **Error handling:** `anyhow::Result` everywhere, `.context("中文描述")` for all IO/network ops
- **CLI framework:** clap 4.5 with derive macros. Subcommands use `#[command(alias = "...")]`
- **Display:** `colored` crate for terminal output. Pattern: `"text".green()`, `"text".bold().cyan()`
- **Async:** tokio rt-multi-thread. All HTTP is async via reqwest
- **No tests currently.** All validation is manual (login, run commands)
- **Zero warnings policy:** Remove unused code rather than suppressing. No `#[allow(dead_code)]`

## Documentation

- `docs/treehole-api.md` — Treehole REST API spec with endpoint details
- `docs/wechat-mp-flow.md` — Step-by-step WeChat MP login flow notes
