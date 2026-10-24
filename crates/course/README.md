<div align="center">

# pku-course

*A command-line client for PKU Teaching Platform (教学网 / Blackboard Learn).*

[![crates.io](https://img.shields.io/crates/v/pku-course?style=flat-square&logo=rust)](https://crates.io/crates/pku-course)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](https://github.com/pkuinfo/pkucli/blob/main/LICENSE)

</div>

Manage your PKU Blackboard Learn courses from the terminal: browse content, track assignments, download files and lecture recordings, and submit homework -- all without opening a browser. Teaching assistants can also post content, create assignments, download submissions and grade them.

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

### Teaching Assistant (助教)

For course staff. Every write only previews by default; add `--yes` to actually write.

```bash
course ta ls <course> [folder]               # Content areas / folder items, incl. hidden
course ta columns <course>                   # Grade Center columns
course ta roster <course>                    # Students (preview accounts excluded)
course ta attempts <course> <column>         # Who submitted, who still needs grading

course ta post <course> 教学内容 slides.pdf   # Publish a file item
course ta assign <course> 课程作业 -t "第六章作业" --due "2026-11-08 23:59" -f hw6.pdf

course ta download <course> <column>         # All submissions + 成绩模板.csv
course ta grade <course> <column> <学号> -s 95 --feedback "推导清楚" -f comments.pdf
course ta grade-batch <course> <column> 成绩模板.csv --files-dir ./feedback

course ta videos <course>                    # 课堂实录 and publish state
course ta publish <course>                   # Publish all unpublished recordings
```

Calculated columns (总计 / 加权总计) are never written, and student-preview accounts (`*_previewuser`) are left out of rosters, downloads and batch grading.

### TOTP (手机令牌)

```bash
course otp bind                  # Bind mobile token
course otp show                  # Display current OTP
```

## How It Works

The student-side commands scrape HTML pages using the [`scraper`](https://crates.io/crates/scraper) crate. Assignment deadlines, course structure, and video URLs are all parsed from the web UI. File uploads use multipart forms.

The `ta` commands use Blackboard's public REST API (`/learn/api/public/v1`), which accepts the same session cookie; writes carry the page's XSRF nonce. Where REST is broken on course.pku.edu.cn (attempt file download, grading with feedback files), they fall back to the classic web forms and verify the result over REST.

## Links

- **Repository**: [github.com/pkuinfo/pkucli](https://github.com/pkuinfo/pkucli)
- **Full documentation**: See the main [README](https://github.com/pkuinfo/pkucli#readme)
- **Claude Code Skill**: [`pku-course` on Clawhub](https://clawhub.com)
