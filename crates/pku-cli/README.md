<div align="center">

# pku-cli

*One install for the complete PKU command-line toolkit.*

[![crates.io](https://img.shields.io/crates/v/pku-cli?style=flat-square&logo=rust)](https://crates.io/crates/pku-cli)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](https://github.com/pkuinfo/pkucli/blob/main/LICENSE)

</div>

A meta-package that installs all PKU command-line tools at once and provides a unified `pku` entry point.

## Install

```bash
cargo install pku-cli
```

This single command installs the `pku` binary along with all sub-tools: `treehole`, `course`, `campuscard`, `elective`, `info-auth`, and `info-spider`.

## Usage

Use the unified `pku` command to access any tool:

```bash
pku treehole login -p        # Login to PKU Treehole
pku course courses --all     # List all Blackboard courses
pku card info                # Campus card balance (alias for campuscard)
pku elective show            # View current course selections
pku auth store               # Store IAAA credentials securely
pku spider search "北大"      # Search WeChat official accounts
```

Or invoke the individual binaries directly:

```bash
treehole list
course assignments
campuscard pay
elective launch
info-auth check
info-spider scrape <url>
```

## First-time Setup

Store your IAAA credentials once (encrypted in your OS keychain):

```bash
pku auth store
```

After that, any tool auto-logs in with `login -p` -- no password prompts.

> [!TIP]
> Run `pku auth check` to see the session status of all services at a glance.

## Included Tools

| Command | Description |
|---------|-------------|
| [`pku treehole`](https://crates.io/crates/pku-treehole) | PKU Treehole anonymous forum |
| [`pku course`](https://crates.io/crates/pku-course) | Blackboard Learn (教学网) |
| [`pku campuscard`](https://crates.io/crates/pku-campuscard) | Campus card (校园卡) |
| [`pku elective`](https://crates.io/crates/pku-elective) | Course selection (选课网) |
| [`pku auth`](https://crates.io/crates/pku-auth) | Credential management |
| [`pku spider`](https://crates.io/crates/pkuinfo-spider) | WeChat article crawler |

## Links

- **Repository**: [github.com/pkuinfo/pkucli](https://github.com/pkuinfo/pkucli)
- **Documentation**: See the main [README](https://github.com/pkuinfo/pkucli#readme) for full usage guide
- **Claude Code Skills**: Available on [Clawhub](https://clawhub.com) — search for `pku-*`
