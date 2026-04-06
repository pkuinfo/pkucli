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

# Lint (zero warnings required — do NOT use #[allow(dead_code)] etc.)
cargo clippy --workspace

# Lint single crate
cargo clippy --bin treehole

# Run a binary
cargo run --bin treehole -- login
cargo run --bin course -- courses --all
cargo run --bin info-spider -- search "人民日报"

# Tests (currently no test files exist)
cargo test --workspace
```

## Architecture

Four crates in a Cargo workspace. Three are CLI binaries; one is a shared library.

```
info-common (lib)          Shared: IAAA auth, session/cookie persistence, QR rendering
    ├── iaaa.rs            PKU unified auth (password + QR code login)
    ├── session.rs         Session/CookieStore JSON persistence → ~/.config/info/<name>/
    └── qr.rs              Terminal QR display (viuer) or system viewer

treehole (bin)             PKU Treehole anonymous forum CLI
    Uses: info-common for IAAA → JWT callback → optional SMS verify
    API: JSON REST at /chapi/api/v3/* (modern) and /chapi/api/* (legacy)

course (bin)               PKU Teaching Network (Blackboard Learn) CLI
    Uses: info-common for IAAA → Blackboard SSO callback
    API: HTML scraping with scraper crate (no JSON API)

info-spider (bin)          WeChat Official Account article crawler
    Standalone: own session.rs, own login flow (WeChat QR, not IAAA)
    Config: ~/.config/info-spider/ (separate from common Store)
```

## Auth Flows

**IAAA (treehole + course):** Both reuse `info_common::iaaa`. Each crate provides its own `IaaaConfig` with a different `app_id` and `redirect_url`:
- treehole: `app_id="PKU Helper"`, redirect to `/chapi/cas_iaaa_login`
- course: `app_id="blackboard"`, redirect to Blackboard SSO endpoint

After IAAA returns a token, each crate has its own `complete_*_login()` that exchanges the token with the target service and saves session+cookies.

**info-spider:** Completely separate WeChat QR login flow. Does not use `info-common`.

## Session & Config Storage

Treehole/course use `info_common::session::Store::new(APP_NAME)` → `~/.config/info/<name>/`:
- `session.json` — token, expires_at, uid, created_at, extra (serde_json::Value)
- `cookies.json` — reqwest CookieStore serialized

info-spider uses its own Store → `~/.config/info-spider/`:
- `session.json` — token, fingerprint, bizuin, created_at
- `cookies.json` — same cookie format

## HTTP Client Pattern

Every crate has a `client.rs` with two builders:
- `build(cookie_store: Arc<CookieStoreMutex>)` — for authenticated requests, persistent cookies
- `build_simple()` — for IAAA login only (internal cookie jar, JSESSIONID handling)

Both set a realistic Chrome User-Agent. Treehole uses `redirect(Policy::none())` for manual redirect handling; others use default policy.

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
