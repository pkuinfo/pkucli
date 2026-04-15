---
name: bdkj
description: "北京大学空间 (bdkj.pku.edu.cn) 学术研讨教室预约 CLI 工具。当用户提及 bdkj、北大空间、学术研讨教室、教室预约、研讨间、借教室 或想要查询/预约/取消 二教/四教/地学 教学楼里的空闲学术研讨教室时使用此 skill。支持保存固定参与人分组（如课题组），重复发起预约时只需 `--group <名字>`。Also use when dealing with bdkj IAAA SSO callback (app_id=bdkj), `/room/classRoom` / `/classRoom/seachStudent` / `/classRoom/handle/submit` 这些端点，或 bdkj groups.json 持久化。"
version: 1.0.0
---

# bdkj - 北大空间学术研讨教室预约 CLI

A CLI client for PKU's academic discussion room reservation system at bdkj.pku.edu.cn.

## Architecture

- **Crate location**: `crates/bdkj/`
- **Auth flow**: IAAA SSO (`app_id="bdkj"`) → `/login/oauth` 回调种 JWTUser/SESSION/rememberMe cookies
- **API**: 混合 JSON (`/room/classRoom`, `/classRoom/seachStudent`, `/classRoom/historyTime`) + HTML form 步骤 (`/classRoom/applyStep`, `/classRoom/handle/submit`) + GET 取消 (`/classRoom/cancelApply/<id>`)
- **HTML 列表**：用 `scraper` 解析 `div#results > div.row`

## Key Source Files

- `src/main.rs` — tokio::main，调用 `pku_bdkj::run()`
- `src/lib.rs` — Clap CLI 定义 + dispatch
- `src/client.rs` — reqwest client 构造
- `src/login.rs` — IAAA 登录 + JWTUser cookie 提取
- `src/api.rs` — 教室/时段/学生/提交/取消/列出的核心 API
- `src/commands.rs` — 各子命令实现
- `src/display.rs` — 终端渲染（colored）
- `src/groups.rs` — 参与人分组持久化（`~/.config/info/bdkj/groups.json`）

## CLI Commands

| Command | 用途 |
|---------|-----|
| `login -p` | IAAA 密码登录（keyring → env → interactive） |
| `status` / `logout` | 会话管理 |
| `rooms <building>` | 列出教学楼下所有教室（二教/四教/地学） |
| `history <room_id>` | 查询教室已被预约时段 |
| `student <serial> <name>` | 查询学生信息 |
| `list` / `ls` | 列出当前用户的预约记录 |
| `reserve --room-id ... --begin ... --end ... --reason ... -g <group>` | 提交一次预约（用分组） |
| `reserve ... -p 学号:姓名 -p ...` | 提交一次预约（手动列参与人） |
| `cancel <apply_id>` | 取消一次预约 |
| `group list` / `show <name>` / `set <name> -p ...` / `remove <name>` | 管理参与人分组 |

`--group` 与 `--participant` 互斥；两者必须至少指定一个。申请人自己也必须包含在参与人列表中。

## Auto-Login for AI Agents

```bash
info-auth check
bdkj login -p              # 读 keyring，不需要密码
bdkj group set lab -p 2200011523:张三 -p 2200011524:李四
bdkj reserve --room-id 924131235851276288 \
  --begin "2026-04-20 14:00:00" --end "2026-04-20 16:00:00" \
  --reason "课题组讨论" -g lab
```

## Development Notes

- 所有文案中文
- 错误用 `anyhow::Result` + `.context("中文描述")`
- Session 持久化 `~/.config/info/bdkj/`；groups 在同目录下 `groups.json`
- `submit_apply` 的成功判据是重定向后 URL path 为 `/classRoom`，而不是扫 `layer.msg`（页面模板里有无关的 layer.msg 调用会误命中）
- 教室/教学楼 ID 需要硬编码的 building 常量（见 `api.rs`）
