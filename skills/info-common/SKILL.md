---
name: info-common
description: "Shared library crate (info-common) providing IAAA authentication, OTP, session persistence, and QR rendering for PKU CLI tools. Use this skill when working on the info-common crate, modifying shared auth logic, debugging IAAA login failures, extending session storage, adding a new CLI tool that needs PKU auth, or when the user mentions IAAA, unified auth, 统一认证, OTP/手机令牌, session persistence, cookie storage, or QR code display. Also use when understanding how treehole/course/campuscard/elective share authentication infrastructure."
version: 1.0.0
---

# Info-Common - 共享认证库

The shared library crate providing authentication, session management, and utilities for all IAAA-based CLI tools.

## Architecture

- **Crate location**: `crates/info-common/`
- **Used by**: treehole, course, campuscard, elective (NOT info-spider)
- **Config root**: `~/.config/info/<name>/` for each consumer crate

## Key Modules

### `iaaa.rs` — IAAA Unified Authentication
- PKU's Single Sign-On system supporting both password and QR code login
- Each consumer provides its own `IaaaConfig` with `app_id` and `redirect_url`:
  - treehole: `app_id="PKU Helper"`, redirect to `/chapi/cas_iaaa_login`
  - course: `app_id="blackboard"`, redirect to Blackboard SSO
  - campuscard: `app_id="portal2017"`, redirect to portal → berserker-auth
  - elective: `app_id="elective"`, redirect to elective SSO
- Returns a token that the consumer exchanges with its target service

### `otp.rs` — TOTP Code Generation
- Implements RFC 6238 (Time-based One-Time Password)
- Used for IAAA 手机令牌 (mobile token) 2FA
- Supports bind/set/show/clear operations across all CLI tools

### `session.rs` — Session & Cookie Persistence
- `Store::new(APP_NAME)` creates storage at `~/.config/info/<name>/`
- `session.json` — token, expires_at, uid, created_at, extra (serde_json::Value)
- `cookies.json` — reqwest CookieStore serialized as JSON
- Handles load/save with proper error context

### `qr.rs` — Terminal QR Code Display
- Renders QR codes in terminal via `viuer` crate
- Falls back to system image viewer if terminal rendering fails
- Used for both IAAA QR login and campuscard payment codes

## Adding a New CLI Tool

To add a new IAAA-based CLI tool:

1. Create a new crate under `crates/`
2. Depend on `info-common` in `Cargo.toml`
3. Define `IaaaConfig` with the service's `app_id` and `redirect_url`
4. Implement `complete_*_login()` to exchange the IAAA token with the target service
5. Use `Store::new("tool-name")` for session persistence
6. Follow the `client.rs` pattern: `build()` for auth requests, `build_simple()` for IAAA login

## Development Conventions

- All user-facing strings in **Chinese**
- Error handling: `anyhow::Result` with `.context("中文描述")`
- HTTP clients use realistic User-Agent headers
- Zero warnings policy: remove unused code, never use `#[allow(dead_code)]`
