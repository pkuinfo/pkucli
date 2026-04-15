---
name: portal
description: "北京大学校内信息门户 (portal.pku.edu.cn / its.pku.edu.cn) CLI 工具。当用户提及 portal、校内信息门户、空闲教室、空教室、教室查询、一教/二教/三教/四教/理教空闲、校历、学年校历、上课时间、网费、网费余额、网费充值、上网账号、查余额、微信充值网费、支付宝充值网费 时使用此 skill。也支持 `portal netfee watch --threshold` 做低余额监测（低于阈值退出码 2，适合 cron 报警）。Also use when dealing with `/publicQuery/classroomQuery/retrClassRoomFree.do`、simso 校历 Vue bundle、its.pku.edu.cn `/cas/webLogin` / `/cas/ITSweb` / `/myConn.jsp` / `/netportal/itsUtil?operation=info`、或 `/paybank/user.PayBankOrderPKU` → cwsf.pku.edu.cn `/PayPreService/pay/cashier/gotToPay` 3 步充值流程。"
version: 1.0.0
---

# portal - 北大校内信息门户 CLI

A CLI client for PKU's campus info portal — 空闲教室、校历、网费 in one command.

## Architecture

- **Crate location**: `crates/portal/`
- **Auth flow**:
  - 空闲教室 / 校历：**无需登录**
  - 网费：its.pku.edu.cn 自己的上网账号密码（**不是** IAAA SSO），复用 `pkuinfo_common::credential`（多数学生上网密码与 IAAA 相同）
- **API**:
  - 空闲教室：`GET /publicQuery/classroomQuery/retrClassRoomFree.do?buildingName=<中文>&time=<中文>`，返回 `{success, rows:[{room, cap, c1..c12}]}`
  - 校历：simso.pku.edu.cn 是 Vue SPA，数据硬编码在 `js/ccSchoolCalendar.<hash>.js` bundle 里；先抓 HTML 取 bundle 文件名，再正则抽 `t._v("...")` 文本
  - 网费状态：`POST /cas/webLogin` → `/netportal/itsUtil?operation=info`（HTML 表含余额）→ `POST /cas/ITSweb cmd=select` → `GET /myConn.jsp`（在线 IP 会话）
  - 网费充值：3 步 `pkuConfirm` → `pkuSendOrder` → cwsf.pku.edu.cn 收银台 AJAX `/PayPreService/pay/cashier/gotToPay` 返回 `{data:{urlCode}}`
- **验证码**：复用 `pkuinfo_common::captcha`（manual / utool 免费 / ttshitu / yunma）

## Key Source Files

- `src/main.rs` — tokio::main 调用 `pku_portal::run()`
- `src/lib.rs` — Clap CLI + dispatch
- `src/client.rs` — reqwest client (simple 和 with cookies 两档)
- `src/freeclassroom.rs` — 空闲教室 API + 渲染
- `src/calendar.rs` — simso Vue bundle 解析
- `src/netfee.rs` — ITS 登录 / 余额 / 会话 / 3 步充值 / 付款码 QR 渲染

## CLI Commands

| Command | 用途 |
|---------|-----|
| `free-classroom <building> [-d today\|tomorrow\|day-after]` / `fc` | 查询空闲教室。building: 一教/二教/三教/四教/理教/文史/哲学/地学/国关/政管 |
| `calendar [-y 2025-2026]` / `cal` | 显示校历（best-effort 从 simso JS bundle 抽取） |
| `netfee status` | 余额 + 在线会话 |
| `netfee recharge <amount> [-m wechat\|alipay] [--captcha utool]` | 充值，终端打印微信/支付宝付款二维码 |
| `netfee watch --threshold 10` | 低余额监测，余额 < 阈值返回退出码 2 |

## Auto-Login for AI Agents

```bash
info-auth check
portal free-classroom 一教            # 无需登录
portal calendar --year 2025-2026      # 无需登录
portal netfee status                  # 需要 keyring 凭据
portal netfee recharge 10 -m wechat   # 微信扫码付款，二维码直接打终端
portal netfee watch -t 5 || notify "PKU netfee low"
```

## Development Notes

- 所有文案中文；错误 `anyhow::Result` + `.context("...")`
- its 的 `/paybank/user.PayBankOrderPKU` 响应页**内嵌的** `alert("验证码不能为空！")` 是 JS 函数体里的 literal，不是真正的错误。只匹配独立 `<script>alert("...");history.back();</script>` 才是真错误
- step1 `paytype=""` 即可；真正的 `payType=02`（微信）/`01`（支付宝）是 step3 收银台 AJAX 的字段
- step2 响应会被重定向到 `cwsf.pku.edu.cn` 域，step3 要用那个域的 origin 而不是 its.pku.edu.cn
- 微信付款二维码用 `qrcode` crate 的 `Dense1x2` 在终端渲染；兜底打印原始 `urlCode` 字符串
- Session 持久化 `~/.config/info/portal/`
- 校历是 best-effort：simso 改版 JS 编译方式就会解析失败，错误信息里会提示用户
