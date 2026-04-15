<div align="center">

# pkuinfo-common

*Shared IAAA authentication and utilities for PKU CLI tools.*

[![crates.io](https://img.shields.io/crates/v/pkuinfo-common?style=flat-square&logo=rust)](https://crates.io/crates/pkuinfo-common)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](https://github.com/pkuinfo/pkucli/blob/main/LICENSE)

</div>

The foundation library behind [`pku-cli`](https://crates.io/crates/pku-cli). Provides IAAA single-sign-on, TOTP, session persistence, credential resolution, and QR code rendering -- everything needed to build a PKU service client.

## Install

```toml
[dependencies]
pkuinfo-common = "0.1"
```

## Modules

| Module | Purpose |
|--------|---------|
| `iaaa` | PKU unified SSO (password + QR login) |
| `otp` | RFC 6238 TOTP code generation for 手机令牌 |
| `session` | JSON-based session & cookie persistence |
| `credential` | Keyring → env var → interactive credential resolution |
| `qr` | Terminal QR code rendering (viuer / system viewer) |

## Example

```rust
use pkuinfo_common::{iaaa, session::Store, credential};

let config = iaaa::IaaaConfig {
    app_id: "your-app-id".into(),
    redirect_url: "https://service.pku.edu.cn/callback".into(),
};

let (username, password) = credential::resolve_credential()?;
let token = iaaa::login_with_password(&config, &username, &password).await?;

let store = Store::new("your-service")?;
store.save_token(&token)?;
```

## Credential Resolution Order

1. **OS Keyring** (`info-pku` service)
2. **Environment variables** (`PKU_USERNAME` + `PKU_PASSWORD`)
3. **Interactive prompt**

Passwords are never written to disk in plaintext -- the keyring uses OS-level encryption (Keychain / GNOME Keyring / Credential Manager).

## Building a New PKU CLI

1. Depend on `pkuinfo-common`
2. Define an `IaaaConfig` with the service's `app_id` and redirect URL
3. Call `credential::resolve_credential()` for login
4. Use `Store::new("your-tool")` for session persistence
5. Follow the existing crates ([`pku-treehole`](https://crates.io/crates/pku-treehole), [`pku-course`](https://crates.io/crates/pku-course), etc.) as reference

## Links

- **Repository**: [github.com/pkuinfo/pkucli](https://github.com/pkuinfo/pkucli)
- **Meta-package**: [`pku-cli`](https://crates.io/crates/pku-cli) -- install all PKU tools at once
