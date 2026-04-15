---
name: course
description: "PKU Teaching Platform (北大教学网/Blackboard Learn) CLI tool built in Rust. Use this skill when working on the course crate, debugging course commands, adding features to the teaching platform CLI, or when the user mentions 教学网, Blackboard, course downloads, assignment submission, video downloads, or PKU teaching. Also use when dealing with HTML scraping via the scraper crate, Blackboard SSO callback, or multipart file uploads. **NOT for course schedule / 课表 / weekly timetable questions — for those use the `treehole` skill (`treehole course`), which is the canonical source and includes 辅修/双学位.** This `course` crate only deals with the Blackboard teaching platform (assignments, files, recordings, announcements)."
version: 2.0.0
---

# Course - 北大教学网 CLI

A CLI client for PKU's Blackboard Learn teaching platform.

## Architecture

- **Crate location**: `crates/course/`
- **Auth flow**: IAAA SSO (`app_id="blackboard"`) → Blackboard SSO endpoint callback
- **API**: HTML scraping with `scraper` crate (no JSON API available)
- **File upload**: multipart form via reqwest

## Key Source Files

- `src/main.rs` — Clap CLI with subcommands
- `src/commands.rs` — Command implementations
- `src/api.rs` — HTML scraping logic, page parsers
- `src/display.rs` — Terminal output formatting
- `src/client.rs` — reqwest client builders

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

## Auto-Login for AI Agents

```bash
# Check session status
info-auth check

# Auto-login (reads credentials from OS keyring, no password needed)
course login -p
```

## Development Notes

- HTML parsing is the primary data extraction method — no REST API
- Assignment list supports deadline-based sorting
- Video download handles replay/recording URLs
- All user-facing strings in **Chinese**
- Error handling: `anyhow::Result` with `.context("中文描述")`
- Session persisted to `~/.config/info/course/`
- Credentials resolved via `info_common::credential` (keyring → env → interactive)
