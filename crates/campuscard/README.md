<div align="center">

# pku-campuscard

*A command-line client for PKU Campus Card (校园卡).*

[![crates.io](https://img.shields.io/crates/v/pku-campuscard?style=flat-square&logo=rust)](https://crates.io/crates/pku-campuscard)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](https://github.com/pkuinfo/pkucli/blob/main/LICENSE)

</div>

Check your campus card balance, show payment QR codes, view transaction history, and see monthly spending breakdowns -- all from the terminal.

## Install

```bash
cargo install pku-campuscard
```

Or install the [complete PKU toolkit](https://crates.io/crates/pku-cli):

```bash
cargo install pku-cli
```

## Setup

Store your IAAA credentials once (encrypted in your OS keychain):

```bash
cargo install pku-auth
info-auth store
```

Then login:

```bash
campuscard login -p
```

## Usage

### Card Info & Payment

```bash
campuscard info                  # Card details & current balance
campuscard pay                   # Show payment QR code in terminal
campuscard pay -o qr.png         # Export payment QR to a PNG file
```

### Recharge

```bash
campuscard recharge              # Interactive amount selection
campuscard recharge -a 100       # Top up 100 yuan
```

### Transaction History

```bash
campuscard bills                 # Recent transactions
campuscard bills -p 2 -n 20      # Page 2, 20 per page
campuscard bills -m 2026-03      # March 2026 only
campuscard ls                    # Short alias for bills
```

### Statistics

```bash
campuscard stats                 # Current month spending breakdown
campuscard stats -m 2026-03      # Specific month
```

### TOTP (手机令牌)

```bash
campuscard otp bind              # Bind mobile token
campuscard otp show              # Display current OTP
```

## How It Works

Campus card data comes from the Synjones platform at `bdcard.pku.edu.cn`. The auth chain is:

```
IAAA SSO → portal2017 → berserker-auth → Synjones JWT
```

The client uses a mobile User-Agent (`PKUANDROID`) and forces HTTP/1.1, as the server does not support HTTP/2.

## Links

- **Repository**: [github.com/pkuinfo/pkucli](https://github.com/pkuinfo/pkucli)
- **Full documentation**: See the main [README](https://github.com/pkuinfo/pkucli#readme)
- **Claude Code Skill**: [`pku-campuscard` on Clawhub](https://clawhub.com)
