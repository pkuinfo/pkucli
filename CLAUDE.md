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
cargo build --release --bin cwfw
cargo build --release --bin bdkj

# Note: the bin name `course` exists in BOTH `pku-course` and `pku-cli`.
# When running, disambiguate with `-p`: `cargo run -p pku-course --bin course -- ...`

# Lint (zero warnings required — do NOT use #[allow(dead_code)] etc.)
cargo clippy --workspace --all-targets -- -D warnings

# Per-crate strict check (typical pre-commit dance for a single crate)
cargo fmt -p pku-bdkj -- --check
cargo check -p pku-bdkj --all-targets
cargo clippy -p pku-bdkj --all-targets -- -D warnings

# Run a binary
cargo run -p pku-treehole --bin treehole -- login
cargo run -p pku-course --bin course -- courses --all
cargo run --bin campuscard -- login
cargo run --bin elective -- login
cargo run --bin info-spider -- search "人民日报"
cargo run --bin info-auth -- store     # interactive credential setup
cargo run --bin info-auth -- check     # check all session status
cargo run --bin info-auth -- status    # check keyring status
cargo run --bin bdkj -- login -p
cargo run --bin bdkj -- list           # list classroom reservations

# Tests (currently no test files exist)
cargo test --workspace
```

## Architecture

11 crates in a Cargo workspace. One shared library (`common`), one meta install bundle (`pku-cli`), the rest are CLI binaries against PKU services.

```
common (pkuinfo-common, lib)    Shared: IAAA auth, OTP, session/cookie persistence, QR, credential resolution
    ├── iaaa.rs            PKU unified auth (password + QR code login)
    ├── otp.rs             TOTP code generation (RFC 6238, for IAAA 手机令牌)
    ├── session.rs         Session/CookieStore JSON persistence → ~/.config/info/<name>/
    ├── credential.rs      Unified credential resolution: session → keyring → env → interactive
    └── qr.rs              Terminal QR display (viuer) or system viewer

info-auth (bin)            Credential management CLI — store/clear/check IAAA credentials
    Uses: common for keyring operations
    Purpose: User runs `info-auth store` once to save credentials to OS keyring;
             AI Agents then call `<tool> login -p` without ever seeing passwords

treehole (bin)             PKU Treehole anonymous forum + academic info CLI
    Uses: common for IAAA → JWT callback → optional SMS verify
    API: JSON REST at /chapi/api/v3/* (modern) and /chapi/api/* (legacy)
    Note: Also exposes `course` / `schedule` / `academic-cal` / `activity-cal` —
          THIS IS THE CANONICAL SOURCE for "课表 / weekly schedule" questions.
          `treehole course` returns a unified weekly grid that includes 主修+辅修+双学位.
          Do NOT answer schedule questions via the `course` (Blackboard) or `elective` crate.

course (bin)               PKU Teaching Network (Blackboard Learn) CLI — assignments, files, recordings
    Uses: common for IAAA → Blackboard SSO callback
    API: HTML scraping with scraper crate (no JSON API), multipart upload
    NOT for course schedule lookups (see treehole).

campuscard (bin)           PKU Campus Card CLI (bdcard.pku.edu.cn, Synjones platform)
    Uses: common for IAAA → portal → berserker-auth → JWT
    API: JSON REST, requires mobile UA + synjones-auth header, HTTP/1.1 only
    Features: payment QR code, recharge, transaction history, monthly stats

elective (bin)             PKU Course Selection (elective.pku.edu.cn) CLI
    Uses: common for IAAA → elective SSO callback
    API: HTML scraping, CAPTCHA handling (base64 image recognition)
    Dual-degree: `--dual major | minor` switches program; `elective show` only sees one
                 program at a time, so it is unsuitable for "this term's full schedule".

cwfw (bin)                 PKU 财务综合信息门户 CLI (cwfw.pku.edu.cn / WF_CWBS)
    Uses: common for IAAA (app_id=IIPF) → home2.jsp → findpages_postData.action → home3.jsp
          → WF_CWBS subsystem entry. Multi-step session bootstrap is essential.
    Features: 个人酬金查询 etc.

bdkj (bin)                 北大空间 — 学术研讨教室预约 (bdkj.pku.edu.cn)
    Uses: common for IAAA (app_id=bdkj) → /login/oauth callback → JWTUser/SESSION cookies
    API: mixed JSON (/room/classRoom, /classRoom/seachStudent, /classRoom/historyTime) +
         HTML form steps (/classRoom/applyStep, /classRoom/handle/submit) +
         GET cancel (/classRoom/cancelApply/<id>). HTML listing scraped with `scraper`
         from `div#results > div.row`.
    Participant groups persisted to `~/.config/info/bdkj/groups.json` so users can
    save a fixed roster (e.g. `bdkj group set phy3 -p 学号:姓名 ...`) and reuse with
    `bdkj reserve --group phy3 ...`. `--group` and `--participant` are mutually exclusive.

info-spider (bin)          WeChat Official Account article crawler
    Standalone: own session.rs, own login flow (WeChat QR, not IAAA)
    Config: ~/.config/info-spider/ (separate from common Store)

claspider (bin)            PKU 课程信息爬取（教务部 + 选课网）— bulk scraping for course catalog data

pku-cli (bin meta)         Meta crate that re-exports the per-service binaries so users can
    `cargo install pku-cli` and get them all at once. Note that the same bin name
    (`course`, `treehole`, ...) is defined in BOTH the original crate and pku-cli, so
    `cargo run --bin <name>` is ambiguous — disambiguate with `-p <package>`.
```

## Auth Flows

**IAAA (treehole + course + campuscard + elective + cwfw + bdkj):** All reuse `pkuinfo_common::iaaa`. Each crate provides its own `IaaaConfig` with a different `app_id` and `redirect_url`:
- treehole: `app_id="PKU Helper"`, redirect to `/chapi/cas_iaaa_login`
- course: `app_id="blackboard"`, redirect to Blackboard SSO endpoint
- campuscard: `app_id="portal2017"`, redirect to portal → berserker-auth → JWT
- elective: `app_id="elective"`, redirect to elective SSO endpoint
- cwfw: `app_id="IIPF"`, redirect to `cwfw.pku.edu.cn/WFManager/home2.jsp` (then 2 more bootstrap hops)
- bdkj: `app_id="bdkj"`, redirect to `http://bdkj.pku.edu.cn/login/oauth`

After IAAA returns a token, each crate has its own `complete_*_login()` that exchanges the token with the target service and saves session+cookies.

**Credential Resolution Order** (in `info_common::credential`):
1. OS Keyring (`info-pku` service) — set by `info-auth store`
2. Environment variables (`PKU_USERNAME` + `PKU_PASSWORD`)
3. Interactive prompt (fallback)

**AI Agent Safety**: Passwords NEVER appear in CLI arguments. `info-auth store` handles all password input interactively. AI Agents should only call `<tool> login -p` which auto-resolves credentials from keyring/env. Use `info-auth check` to verify session status.

**info-spider:** Completely separate WeChat QR login flow. Does not use `info-common`.

## Session & Config Storage

Treehole/course/campuscard/elective/cwfw/bdkj use `pkuinfo_common::session::Store::new(APP_NAME)` → `~/.config/info/<name>/`:
- `session.json` — token, expires_at, uid, created_at, extra (serde_json::Value)
- `cookies.json` — reqwest CookieStore serialized
- Per-crate ad-hoc state may live alongside in the same directory (e.g. `bdkj/groups.json`).
  Always put new persistent state under `~/.config/info/<crate>/`, not a sibling directory.

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
- **Skill routing:** Skill descriptions in `skills/<name>/SKILL.md` actively route incoming
  questions. The `treehole` skill claims 课表/schedule/calendar; `course` and `elective`
  explicitly disclaim them. Update the relevant SKILL.md when responsibilities shift, or
  duplicate skills will get triggered for the same question.

## Documentation

- `docs/treehole-api.md` — Treehole REST API spec with endpoint details
- `docs/wechat-mp-flow.md` — Step-by-step WeChat MP login flow notes
