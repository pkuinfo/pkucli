---
name: disk
description: "北京大学网盘 (disk.pku.edu.cn，爱数 AnyShare 7) CLI 工具，只读：浏览和下载。当用户提及 北大网盘、PKU 网盘、disk.pku.edu.cn、AnyShare、个人文档库、其他文档库（别人共享给我的）、部门文档库、北大网盘公共资源，或想要 列出/查找/下载 网盘里的文件或整个文件夹 时使用此 skill。Also use when dealing with the AnyShare OAuth2 (Ory Hydra) login chain, IAAA app_id=anyshare, `/api/efast/v1/entry-doc-lib` / `dir/list` / `file/osdownload`, or token refresh via `/anyshare/oauth2/login/refreshToken`."
version: 1.0.0
---

# disk - 北大网盘 CLI

Read-only client for PKU's netdisk at disk.pku.edu.cn (Eisoo AnyShare 7.0.1.5).

## Architecture

- **Crate location**: `crates/disk/`
- **Auth flow**: AnyShare web OAuth2 (Ory Hydra) + IAAA third-party auth (`PkuAuth`):
  1. `GET /anyshare/oauth2/login?redirect=/anyshare/` → `/oauth2/auth` → `/oauth2/signin?login_challenge=C`。`redirect` 决定 `state`；不带它时 state 是 `"undefined"`，回调会 400「state验证失败」
  2. 把 C 写进 `login_challenge` cookie（网页端「统一认证登录」按钮就是这么做的）
  3. IAAA 登录，`app_id="anyshare"`，redirect `https://disk.pku.edu.cn/oauth2/signin`
  4. `GET /oauth2/signin?_rand=…&token=<iaaa_token>` → Hydra login/consent 自动通过 → `/anyshare/oauth2/login/callback`，种下 `client.oauth2_token`、`client.oauth2_refresh_token`、`id_token`
- **API**: JSON REST, `Authorization: Bearer <client.oauth2_token>`
  - `GET /api/efast/v1/entry-doc-lib`：顶层文档库，type 有 `user_doc_lib`（个人文档库）、`shared_user_doc_lib`（其他文档库 = 别人共享给我的个人文档）、`department_doc_lib`（部门文档库）、`custom_doc_lib`（自定义文档库）、`knowledge_doc_lib`（知识库）。不带 `?type=` 时全部返回；未知类型也会原样显示，不能丢
  - `POST /api/efast/v1/dir/list {docid, sort, by}` → `{dirs, files}`，文件夹 size 为 -1，时间戳是微秒
  - `POST /api/efast/v1/file/osdownload {docid, authtype:"QUERY_STRING"}` → `authrequest = [方法, 预签名 URL, "Header: v"...]`，对象存储在 `diskasu.pku.edu.cn:10002`，直接 GET 即可
- **Token refresh**: 访问令牌 1 小时过期。遇到 401 时 `GET /anyshare/oauth2/login/refreshToken?isforced=true`（带 cookie），返回 `{"code":200,"message":"ok"}` 并写回三个新 cookie；不带 `isforced` 时令牌未过期只返回 `active`。刷新会作废旧刷新令牌，必须立即落盘

## Key Source Files

- `src/lib.rs` — Clap CLI 定义 + dispatch
- `src/login.rs` — IAAA + OAuth2 登录链路（逐跳跟随重定向，cookie 落同一个 jar）
- `src/api.rs` — 文档库 / 列目录 / 下载，401 自动续期
- `src/commands.rs` — 路径解析、ls、download（递归、跳过已存在）
- `src/display.rs` — 终端输出
- `src/client.rs` — reqwest client（登录用不跟随重定向的版本）

## CLI Commands

| Command | 用途 |
|---------|-----|
| `login -p` / `login` | IAAA 密码登录（keyring → env → interactive）/ 扫码登录 |
| `status` / `logout` | 会话管理 |
| `ls` | 列出全部文档库，按类型分组并编号 |
| `ls <路径>` | 列出文件夹内容（或显示单个文件信息） |
| `download <路径> [-o 目录]` / `dl` | 下载文件或整个文件夹，保持目录结构，大小相同的已有文件跳过 |

路径第一段是文档库，可以写：完整名字（如 `北大网盘公共资源`）、`disk ls` 里的序号、`个人`/`我的`（自己的个人文档库，真实名字是「姓名_学号」）、类型名（如 `部门文档库`，该类型只有一个时）、或名字的唯一片段（如老师的名字）。后面各段按名字逐级匹配。

## Auto-Login for AI Agents

```bash
disk login -p                       # 读 keyring，不需要密码
disk ls                             # 先看有哪些文档库
disk ls 其他文档库                   # 只有一个其他文档库时可以用类型名
disk ls 个人/课件
disk download "个人/课件/第一章.pdf" -o ./out
disk download 王老师/课程资料 -o ./out           # 别人共享的整个文件夹（用名字片段选文档库）
```

## Development Notes

- 只读：没有上传、删除、移动、分享（这些在 AnyShare 里是 `file/osbeginupload` 等多步接口，尚未实现）
- 所有文案中文；错误用 `anyhow::Result` + `.context("中文描述")`
- Session 持久化 `~/.config/info/disk/`（session.json + cookies.json，令牌都在 cookie 里）
- 下载先写 `.part` 再改名，避免半截文件被当成已完成而跳过
