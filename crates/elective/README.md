<div align="center">

# pku-elective

*A command-line client for PKU Course Selection (选课网) with auto-enrollment.*

[![crates.io](https://img.shields.io/crates/v/pku-elective?style=flat-square&logo=rust)](https://crates.io/crates/pku-elective)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](https://github.com/pkuinfo/pkucli/blob/main/LICENSE)

</div>

Automate PKU's course add/drop period. Define your target courses, configure a CAPTCHA solver backend, and let the tool poll for open slots and enroll you automatically.

## Install

```bash
cargo install pku-elective
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
elective login -p                    # Single-degree students
elective login -p --dual major       # Dual-degree: major
elective login -p --dual minor       # Dual-degree: minor
```

> [!IMPORTANT]
> Dual-degree students **must** specify `--dual major` or `--dual minor`.
> Single-degree students should omit the flag.

## Usage

### Browse Your Selections

```bash
elective show                    # View current course selections
elective list                    # Browse available courses for add/drop
elective list -p 2               # Page 2
```

### Auto-Enrollment Setup

```bash
elective set                     # Interactive: pick target courses
elective unset                   # Remove a target
```

### CAPTCHA Backend

```bash
elective config-captcha manual   # Display image, user types code
elective config-captcha utool    # UTool OCR service
elective config-captcha ttshitu  # TTShiTu recognition API
elective config-captcha yunma    # Yunma recognition API
```

### Run the Auto-Enroll Loop

```bash
elective launch                  # Default 15s interval
elective launch -t 10            # Check every 10 seconds
```

The loop continuously polls for open slots in your target courses and attempts enrollment. It will continue running until all targets are enrolled or you press Ctrl+C.

### TOTP (手机令牌)

```bash
elective otp bind                # Bind mobile token
elective otp show                # Display current OTP
```

## How It Works

The elective system uses HTML scraping for course data and base64-encoded images for CAPTCHAs. The auto-enroll loop respects the server's pace to avoid rate limiting while maximizing your chance of catching an open slot.

## Links

- **Repository**: [github.com/pkuinfo/pkucli](https://github.com/pkuinfo/pkucli)
- **Full documentation**: See the main [README](https://github.com/pkuinfo/pkucli#readme)
- **Claude Code Skill**: [`pku-elective` on Clawhub](https://clawhub.com)
