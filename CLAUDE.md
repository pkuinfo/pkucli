# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

Linking uses mold via clang (`.cargo/config.toml`; delete its target table if mold/clang are missing). Dev builds compile dependencies at opt-level 1 without debug info and build scripts without debug info, which keeps `target/` around 2 GB. `cargo trim` (alias) drops dev/check artifacts and keeps release binaries.

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
cargo build --release --bin cwpay

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
cargo run --bin bdkj -- rooms 二教      # discussion rooms (from /space)
cargo run --bin bdkj -- schedule <room_id> --days 3   # free periods
cargo run --bin portal -- free-classroom 一教 --day today
cargo run --bin portal -- netfee status

# Tests (currently no test files exist)
cargo test --workspace
```

## Architecture

13 crates in a Cargo workspace. One shared library (`common`), one meta install bundle (`pku-cli`), the rest are CLI binaries against PKU services.

```
common (pkuinfo-common, lib)    Shared: IAAA auth, OTP, session/cookie persistence, QR, credential resolution, captcha, TLS
    ├── iaaa.rs            PKU unified auth (password + QR code login)
    ├── otp.rs             TOTP code generation (RFC 6238, for IAAA 手机令牌)
    ├── session.rs         Session/CookieStore JSON persistence → ~/.config/info/<name>/
    ├── credential.rs      Unified credential resolution: session → keyring → env → interactive
    ├── qr.rs              Terminal QR display (viuer) or system viewer
    ├── captcha.rs         Pluggable CAPTCHA recognition: Manual / Utool (free) / TTShiTu / 云码
    └── tls.rs             Embedded GlobalSign AlphaSSL CA 2025 intermediate cert — fixes broken
                           chain on course.pku.edu.cn (server omits correct intermediate); call
                           `tls::apply_extra_roots(builder)` in every crate's client.rs

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
    API: HTML scraping with scraper crate, multipart upload
    TA (`course ta`, src/ta.rs): Blackboard public REST /learn/api/public/v1 with the same
         session cookie; writes need `X-Blackboard-XSRF` = the page's NonceUtil.nonce.ajax.
         REST attempt-file download and `gradebook/attempts/{id}` are 404 on this server, so
         downloads use /webapps/assignment/download and grading posts the classic
         gradeAttemptForm (skip disabled inputs!) then reads back via
         gradebook/columns/{col}/attempts/{id}. 课堂实录 publish = GET publishVideo.action /
         unpublishVideo.action links from the modify-mode list. Writes preview unless --yes;
         calculated columns refused; *_previewuser accounts excluded from lists and batches.
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
    Features (read-only): cj 个人酬金, gz 个人工资/奖助学金, rk 我的入卡, bx 智能报销概览 (YBX
          subsystem via `login::enter_subsystem`), jk 借款, wh 外汇到款, hl 汇率, xc 薪酬劳务, menu.
    Protocol: generic Wingsoft layer in `wf.rs` (loadDefinition → per-session proc hash →
          responseButtonEvent with form fields in userContext → doQuery); flows in `queries.rs`.

cwpay (bin)                北京大学收费平台 (cwpay.pku.edu.cn, "收费大厅") — read-only
    Uses: common for IAAA (app_id from public `getIAAALoginParam`, now CWSF) → GET /dealcas?token=…
          → 302 `/#/middle_page?token=<X-Token>`; every API call `/api/<path>` + `X-Token` header.
          Responses AES-256-ECB encrypted (crypto.rs); messageCode -3 = renewed token (replay).
    Features: tuition, pending, orders, order <no> (+ e-invoice link), spending, projects, notice, me.
    All parsing in `parse.rs` / `crypto.rs` with unit tests on synthetic fixtures (`fixtures/`).

bdkj (bin)                 北大空间 — 学术研讨教室预约 (bdkj.pku.edu.cn)
    Uses: common for IAAA (app_id=bdkj) → GET /login/oauth?_rand=…&token=… → 302 /home,
          `SESSION` cookie. Site redesigned 2026-10: `/personal/user` is 404 for everyone, so a
          valid session = final URL not on /oauth, /login* (except /login/oauth) or IAAA.
    API (2026 site): rooms scraped from `GET /space` (`[data-building-id]` panels,
         `div.sp-card[data-room-id]` cards); `GET /space/lockStatus` (XHR, `{roomId: 0|1|2}`,
         informative); `GET /space/schedule?roomId=&startDate=` (XHR, taken slots for the
         week; 13 periods/day 08:00–22:30, max 2 consecutive); `GET /classRoom` HTML list
         (`div#results`, or `div.cr-empty` 暂无申请记录). Old `/room/classRoom` and
         `/classRoom/historyTime` are gone. Student check (/classRoom/seachStudent), submit
         (/classRoom/handle/submit) and cancel (/classRoom/cancelApply/<id>) are the old flow,
         unverified on the new site; an incomplete profile redirects to /personal (surfaced as
         a 「个人信息」 error). All parsing is pure functions in `parse.rs` with unit tests.
    Participant groups persisted to `~/.config/info/bdkj/groups.json` so users can
    save a fixed roster (e.g. `bdkj group set phy3 -p 学号:姓名 ...`) and reuse with
    `bdkj reserve --group phy3 ...`. `--group` and `--participant` are mutually exclusive.

info-spider (bin)          WeChat Official Account article crawler
    Standalone: own session.rs, own login flow (WeChat QR, not IAAA)
    Config: ~/.config/info-spider/ (separate from common Store)

claspider (bin)            PKU 课程信息爬取（教务部 + 选课网）— bulk scraping for course catalog data

portal (bin)               PKU 校内信息门户 CLI (portal.pku.edu.cn)
    Auth: independent username/password + CAPTCHA (does NOT use IAAA)
    Features: `portal free-classroom <楼>` — 空闲教室查询
              `portal calendar` — 校历显示（simso Vue bundle 解析，兼容 webpack 与 2026 Vite 构建）
              `portal netfee status/recharge/watch` — 网费查询/充值/低余额监控
    CAPTCHA: uses `common::captcha` (Utool default, configurable)
    Config: ~/.config/info/portal/ (follows common Store convention)

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
- elective: `app_id="syllabus"`, redirect to `http://elective.pku.edu.cn:80/elective2008/ssoLogin.do`
- cwfw: `app_id="IIPF"`, redirect to `cwfw.pku.edu.cn/WFManager/home2.jsp` (then 2 more bootstrap hops)
- bdkj: `app_id="bdkj"`, redirect to `https://bdkj.pku.edu.cn/login/oauth`
- cwpay: `app_id="CWSF"`, redirect to `http://cwpay.pku.edu.cn/dealcas` (X-Token from the dealcas redirect)

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

Both set a realistic User-Agent. All builders call `tls::apply_extra_roots()` from `pkuinfo_common::tls` to inject the embedded AlphaSSL CA cert. Treehole uses `redirect(Policy::none())` for manual redirect handling. Campuscard uses mobile UA (`PKUANDROID`) and requires `http1_only()` (server doesn't support HTTP/2).

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
