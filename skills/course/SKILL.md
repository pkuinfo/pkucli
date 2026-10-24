---
name: course
description: "PKU Teaching Platform (北大教学网/Blackboard Learn) CLI tool built in Rust. Use this skill when working on the course crate, debugging course commands, adding features to the teaching platform CLI, or when the user mentions 教学网, Blackboard, course downloads, assignment submission, video downloads, or PKU teaching. Also use for teaching-assistant (助教) work via `course ta`: posting files, creating assignments, downloading submissions, grading with feedback (登分/批改/成绩中心), and publishing 课堂实录 recordings. Also use when dealing with HTML scraping via the scraper crate, Blackboard SSO callback, or multipart file uploads. **NOT for course schedule / 课表 / weekly timetable questions — for those use the `treehole` skill (`treehole course`), which is the canonical source and includes 辅修/双学位.** This `course` crate only deals with the Blackboard teaching platform (assignments, files, recordings, announcements)."
version: 2.0.0
---

# Course - 北大教学网 CLI

A CLI client for PKU's Blackboard Learn teaching platform.

## Architecture

- **Crate location**: `crates/course/`
- **Auth flow**: IAAA SSO (`app_id="blackboard"`) → Blackboard SSO endpoint callback
- **API**: HTML scraping with `scraper` crate for the student-side commands; the `ta` commands use Blackboard's public REST API (`/learn/api/public/v1`, authenticated by the session cookie, writes need an `X-Blackboard-XSRF` header) plus classic pages where REST is broken on this server
- **File upload**: multipart form via reqwest

## Key Source Files

- `src/main.rs` — Clap CLI with subcommands
- `src/commands.rs` — Command implementations
- `src/api.rs` — HTML scraping logic, page parsers
- `src/display.rs` — Terminal output formatting
- `src/client.rs` — reqwest client builders
- `src/ta.rs` — TA-side API: REST client + XSRF, classic grading form, 课堂实录 publish state

## CLI Commands

| Command | Alias | Function |
|---------|-------|----------|
| `login` / `logout` / `status` | | IAAA → Blackboard auth |
| `courses` | | List enrolled courses (supports `--all` for all semesters) |
| `info` | | Course details |
| `content` | | Browse course content tree |
| `assignments` | | List assignments with deadlines |
| `assignment` | | View single assignment |
| `browse` | | Interactive course content browser |
| `assignment-download` | `adl` | Download assignment attachments |
| `download` | | Download course files |
| `video-download` | `vdl` | Download course recordings |
| `videos` | `vls` | List available videos |
| `submit` | | Upload homework files |
| `otp` | | TOTP 2FA management |
| `ta` | | Teaching-assistant tools (see below) |

## Teaching Assistant (`course ta`)

| Command | Function |
|---------|----------|
| `ta ls <course> [folder]` | List content areas / folder items, including hidden ones |
| `ta columns <course>` | Grade Center columns (calculated columns marked read-only) |
| `ta roster <course>` | Students, excluding `*_previewuser` accounts |
| `ta attempts <course> <column>` | Submission and grading status per student |
| `ta download <course> <column>` | Download all submissions + write a CSV grade template |
| `ta post <course> <folder> <file>` | Publish a file item |
| `ta assign <course> <folder> -t <title> --due <time>` | Create an assignment (with grade column) |
| `ta grade <course> <column> <student> -s <score>` | Grade one attempt, with `--feedback` text and `-f` feedback files |
| `ta grade-batch <course> <column> <csv>` | Grade from the CSV template (`--files-dir` for feedback files) |
| `ta videos` / `ta publish` / `ta unpublish` | 课堂实录 publish state |
| `ta rm <course> <content_id>` | Delete an item (refused if students have submitted) |

Safety rules built into the commands:

- Every write only previews by default; add `--yes` to write. Agents should show the preview to the user before re-running with `--yes`.
- Calculated columns (总计 / 加权总计) are never written.
- Rosters, downloads and batch grading skip `*_previewuser` accounts; single `ta grade` only accepts a preview account when it is named explicitly.
- Grades are verified by reading the attempt back over REST after submitting the classic form.

## Auto-Login for AI Agents

```bash
# Check session status
info-auth check

# Auto-login (reads credentials from OS keyring, no password needed)
course login -p
```

## Development Notes

- HTML parsing is the primary data extraction method for student-side commands; `ta` uses REST where it works
- REST quirks on course.pku.edu.cn: `gradebook/attempts/{id}` is 404 (use `gradebook/columns/{col}/attempts/{id}`), attempt file download is 404 (use `/webapps/assignment/download`), user `name.family` holds the 院系 and `name.given` the full name
- Classic form parsing must skip `disabled` controls: the grading page has disabled `feedbackFiles_remove` inputs that would delete existing feedback files
- Assignment list supports deadline-based sorting
- Video download handles replay/recording URLs
- All user-facing strings in **Chinese**
- Error handling: `anyhow::Result` with `.context("中文描述")`
- Session persisted to `~/.config/info/course/`
- Credentials resolved via `info_common::credential` (keyring → env → interactive)
