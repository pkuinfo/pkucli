<div align="center">

# pku-course

*A command-line client for PKU Teaching Platform (教学网 / Blackboard Learn).*

[![crates.io](https://img.shields.io/crates/v/pku-course?style=flat-square&logo=rust)](https://crates.io/crates/pku-course)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](https://github.com/pkuinfo/pkucli/blob/main/LICENSE)

</div>

Manage your PKU Blackboard Learn courses from the terminal: browse content, track assignments, download files and lecture recordings, and submit homework -- all without opening a browser.

## Install

```bash
cargo install pku-course
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
course login -p
```

## Usage

### Courses

```bash
course courses                   # Current semester courses
course courses --all             # All courses (including past semesters)
course info <course>             # Course sidebar entries
course browse                    # Interactive course content browser
course announcements             # View course announcements
```

### Assignments

```bash
course assignments               # Pending assignments (sorted by deadline)
course assignments --all         # Include completed
course assignments --all-term    # Include past semesters
course assignment <course> <id>  # View single assignment

course assignment-download       # Interactive download of attachments
course adl <hash-id>             # Short alias, download by ID
```

### Submit Homework

```bash
course submit                    # Interactive: pick course + file
course submit <course> <id> <file>
```

### Lecture Recordings

```bash
course videos                    # List all available videos
course videos <course>           # For a specific course
course video-download <id>       # Download by ID
course vdl <id>                  # Short alias
```

### Downloads

```bash
course download <url>            # Download any file by URL
course download <url> -o ./out   # Custom output directory
```

### TOTP (手机令牌)

```bash
course otp bind                  # Bind mobile token
course otp show                  # Display current OTP
```

## How It Works

PKU's Blackboard Learn has no public API, so this tool scrapes HTML pages using the [`scraper`](https://crates.io/crates/scraper) crate. Assignment deadlines, course structure, and video URLs are all parsed from the web UI. File uploads use multipart forms.

## Links

- **Repository**: [github.com/pkuinfo/pkucli](https://github.com/pkuinfo/pkucli)
- **Full documentation**: See the main [README](https://github.com/pkuinfo/pkucli#readme)
- **Claude Code Skill**: [`pku-course` on Clawhub](https://clawhub.com)
