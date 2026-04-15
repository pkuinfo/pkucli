<div align="center">

# pku-treehole

*A command-line client for PKU Treehole (北大树洞) anonymous forum.*

[![crates.io](https://img.shields.io/crates/v/pku-treehole?style=flat-square&logo=rust)](https://crates.io/crates/pku-treehole)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](https://github.com/pkuinfo/pkucli/blob/main/LICENSE)

</div>

Full-featured CLI for browsing, posting, searching, and interacting with the PKU Treehole anonymous forum. Also provides access to your grades, schedule, and academic calendar.

## Install

```bash
cargo install pku-treehole
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
treehole login -p
```

> [!NOTE]
> First-time login may require SMS verification. Set `PKU_SMS_CODE=123456` as an
> environment variable if running non-interactively, or enter it when prompted.

## Usage

### Browse & Search

```bash
treehole list                    # Latest posts (supports --page, -n)
treehole list follow             # Posts you follow
treehole show 12345              # View a post with replies
treehole search "关键词"          # Full-text search
treehole search "#12345"         # Search by post ID
```

### Post & Interact

```bash
treehole post -t "Hello world"         # Create a post
treehole post -t "..." --tag 1,2       # With tags
treehole post -t "..." -i image.jpg    # With an image
treehole post -t "..." --reward 5      # Reward 5 树叶 (tree leaves)

treehole reply 12345 -t "Nice post!"   # Reply to a post
treehole like 12345                    # Upvote
treehole star 12345                    # Bookmark
treehole stars                         # List bookmarks
```

### Messages & Profile

```bash
treehole msg                     # Notifications
treehole read 1 2 3              # Mark messages as read
treehole me                      # Your profile
treehole me --posts              # Your posts
```

### Academic Info

```bash
treehole score                   # View your grades
treehole score -s 25-26-1        # Specific semester
treehole course                  # Weekly course schedule
treehole schedule                # This week's agenda
treehole academic-cal            # Academic calendar
```

### TOTP (手机令牌)

```bash
treehole otp bind                # Bind mobile token (interactive, needs SMS)
treehole otp show                # Display current OTP code
treehole otp clear               # Remove saved OTP config
```

## Links

- **Repository**: [github.com/pkuinfo/pkucli](https://github.com/pkuinfo/pkucli)
- **Full documentation**: See the main [README](https://github.com/pkuinfo/pkucli#readme)
- **Claude Code Skill**: [`pku-treehole` on Clawhub](https://clawhub.com)
