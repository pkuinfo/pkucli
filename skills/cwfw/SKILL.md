---
name: cwfw
description: "北京大学财务综合信息门户 (cwfw.pku.edu.cn / WF_CWBS) CLI 工具。当用户提及 cwfw、财务门户、财务综合信息门户、个人酬金、工资查询、报销查询 时使用此 skill。Also use when dealing with cwfw IAAA 登录 (app_id=IIPF)、home2.jsp→findpages_postData.action→home3.jsp 多步 bootstrap、WF_CWBS 子系统入口、或 cwfw 的加密表单字段。"
version: 1.0.0
---

# cwfw - 北大财务综合信息门户 CLI

A CLI client for PKU's financial information portal at cwfw.pku.edu.cn (WF_CWBS subsystem).

## Architecture

- **Crate location**: `crates/cwfw/`
- **Auth flow**: IAAA SSO (`app_id="IIPF"`) → `cwfw.pku.edu.cn/WFManager/home2.jsp` → `findpages_postData.action` → `home3.jsp` → WF_CWBS 子系统 entry（3 步 bootstrap 缺一不可）
- **API**: HTML 抓取 + 加密的 form 字段

## Key Source Files

- `src/main.rs` — tokio::main 调用 `pku_cwfw::run()`
- `src/lib.rs` — Clap CLI 定义
- `src/client.rs` — reqwest client
- `src/login.rs` — IAAA → multi-step bootstrap → WF_CWBS session
- `src/context.rs` — 会话上下文（子系统 URL 等）
- `src/encrypt.rs` — 表单字段加密（用于某些查询请求）
- `src/api.rs` — 各查询 API
- `src/commands.rs` — 子命令实现
- `src/display.rs` — 终端渲染

## CLI Commands

| Command | 用途 |
|---------|-----|
| `login -p` | IAAA 登录 + cwfw 多步 bootstrap |
| `status` / `logout` | 会话管理 |
| 个人酬金 / 工资 / 报销查询 | 详见 `--help` |

## Auto-Login for AI Agents

```bash
info-auth check
cwfw login -p
cwfw <query-cmd>
```

## Development Notes

- 多步 bootstrap 必须严格顺序执行，否则后续子系统访问会返回登录页
- Session 持久化 `~/.config/info/cwfw/`
- 所有文案中文，`anyhow::Result` + `.context("中文描述")`
- 某些表单字段需要加密（见 `encrypt.rs`），算法直接抄自网页 JS
