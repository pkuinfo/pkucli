<div align="center">

# pku-disk

*A command-line client for PKU Netdisk (北大网盘, disk.pku.edu.cn).*

[![crates.io](https://img.shields.io/crates/v/pku-disk?style=flat-square&logo=rust)](https://crates.io/crates/pku-disk)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](https://github.com/pkuinfo/pkucli/blob/main/LICENSE)

</div>

Browse and download your PKU Netdisk files from the terminal: your personal library, libraries other people shared with you, and department libraries.

## Install

```bash
cargo install pku-disk
```

Or install the [complete PKU toolkit](https://crates.io/crates/pku-cli):

```bash
cargo install pku-cli
```

## Setup

Store your IAAA credentials once (encrypted in your OS keychain), then log in:

```bash
cargo install pku-auth
info-auth store
disk login -p
```

## Usage

```bash
disk ls                                   # All document libraries, grouped by type
disk ls 个人                              # Your personal library
disk ls 个人/课件                         # A folder
disk ls 部门文档库                        # The department library (when there is one)
disk ls 2                                 # Library #2 from `disk ls`

disk download "个人/课件/第一章.pdf"       # One file into the current directory
disk download 个人/课件 -o ./backup        # A whole folder, keeping its structure
disk dl 个人/课件 -o ./backup              # Re-run: files of the same size are skipped
```

The first path segment picks a library: its full name, its number from `disk ls`, `个人` / `我的` for your own library, a type name such as `部门文档库` or `其他文档库` when you have only one of that type, or a unique part of its name.

Library types shown by `disk ls`:

| Type | Meaning |
|------|---------|
| 个人文档库 | Your own files |
| 其他文档库 | Other people's personal libraries shared with you |
| 部门文档库 | Department libraries, e.g. 北大网盘公共资源 |
| 自定义文档库 | Custom libraries |
| 知识库 | Knowledge libraries |

## How It Works

disk.pku.edu.cn runs Eisoo AnyShare 7. Login goes through AnyShare's OAuth2 server with IAAA as the third-party identity provider (`app_id=anyshare`), so it reuses the shared IAAA login of this toolkit (password, QR code and OTP). The access token lasts one hour and is refreshed automatically. Files are listed through AnyShare's `/api/efast/v1` REST API and downloaded from pre-signed object-storage URLs.

The tool is read-only: it never uploads, deletes, moves or shares anything.

## Links

- **Repository**: [github.com/pkuinfo/pkucli](https://github.com/pkuinfo/pkucli)
- **Full documentation**: See the main [README](https://github.com/pkuinfo/pkucli#readme)
