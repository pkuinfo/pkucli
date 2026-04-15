---
name: claspider
description: "北京大学课程信息爬取 CLI (dean.pku.edu.cn 教务部 + elective.pku.edu.cn 选课网 + onlineroomse.pku.edu.cn 智云课堂)。当用户提及 claspider、课程爬虫、课程信息爬取、课程目录、全校开课、按院系/教师/关键词查课、合并课程数据 时使用此 skill。适用于要批量导出某学期全部课程（课号/名称/学分/教师/时间地点/简介）做离线查询、语义搜索、选课决策的场景。Also use when dealing with 教务部课表查询 HTML 抓取、选课网分类遍历、智云课堂 JWT token 周课表接口、或三方数据合并。**不是**选课/退课工具——那是 `elective` skill；也不是单人课表——那是 `treehole course` (treehole skill)。"
version: 1.0.0
---

# claspider - 北大课程信息爬取 CLI

A CLI scraper that pulls course catalog data from multiple PKU sources and merges them into a queryable JSON export.

## Architecture

- **Crate location**: `crates/claspider/`
- **数据源**：
  - **教务部课表查询** (`dean.pku.edu.cn`)——无需登录，HTML 抓取，覆盖全校所有开课
  - **选课系统** (`elective.pku.edu.cn`)——需要 IAAA 登录（复用 `pku-elective` crate 的 session），按课程分类遍历；能拿到选课网特有的备注、教学安排、课程简介
  - **智云课堂** (`onlineroomse.pku.edu.cn`)——需要浏览器 `_token` cookie（JWT），按周查询有直播/录播的课程
- **合并**：同一门课在三个源里的字段互补，`merge` 子命令把它们按课号对齐，产出一份最全的 JSON
- **无自己的 session 存储**；选课相关的登录状态直接复用 `~/.config/info/elective/`

## Key Source Files

- `src/main.rs` — tokio::main 调用 `pku_claspider::run()`
- `src/lib.rs` — Clap CLI 定义 + dispatch
- `src/dean.rs` — 教务部 HTML 抓取
- `src/elective_query.rs` — 选课网抓取（复用 `pku_elective::client_build`）
- `src/zhiyun.rs` — 智云课堂 JWT API
- `src/model.rs` — 统一的 `Course` 结构 + 合并算法
- `src/display.rs` — 终端渲染

## CLI Commands

| Command | 用途 |
|---------|-----|
| `dean --term 25-26-2 [--dept 00048] [--keyword ...] [--teacher ...] [--json]` | 从教务部抓课（无需登录） |
| `elective --category speciality [--dept ...] [--keyword ...] [--json]` | 从选课网抓课（需先 `elective login -p`） |
| `zhiyun --token <JWT> --week-start 2026-04-13 [--detail] [--json]` | 从智云课堂抓有直播/录播的课 |
| `merge --term 25-26-2 --category speciality [--dept ...] [--zhiyun-token ...] [--zhiyun-week ...] [--json]` | 三方合并 |

选课网分类取值：`speciality`（专业课）/ `politics` / `english` / `gym` / `tsk_choice`（通选）/
`pub_choice` / `liberal_computer` / `ldjyk` / `szxzxbx` / `education_plan_bk`。

## 典型用法

```bash
# 纯教务部抓本学期信科全部开课，导出 JSON
claspider dean --term 25-26-2 --dept 00048 --json > info.json

# 选课网补充（要求已 elective login）
elective login -p
claspider elective --category speciality --dept 00048 --json > info_elective.json

# 智云课堂：从浏览器拿 _token cookie
claspider zhiyun --token eyJhbGc... --week-start 2026-04-13 --json > info_zhiyun.json

# 三方合并
claspider merge --term 25-26-2 --category speciality --dept 00048 \
  --zhiyun-token eyJhbGc... --zhiyun-week 2026-04-13 --json > info_merged.json
```

## Development Notes

- 所有文案中文；错误 `anyhow::Result` + `.context("...")`
- dean 源是最稳定的（HTML 结构稳定、无需登录），合并的主干以它为准
- 选课网抓取通过 `pku_elective::client_build(cookie_store)` 复用 elective crate 的 reqwest client 工厂，
  避免重复实现 IAAA 登录；但 claspider 本身**没有**自己的 session 目录
- 智云 token 不是 IAAA 产物，是智云前端自己的 JWT，只能手动从 `onlineroomse.pku.edu.cn` 浏览器
  cookie 里复制，没有 CLI 登录入口
- 合并算法 key 是**课号**（含班号），在 `src/model.rs` 里实现

## 和其他 crate 的区别

- **不是** `elective`（选课/退课工具）——claspider 只读不写，不调用 elect/drop
- **不是** `treehole course`（查单个学生本学期的课表）——claspider 抓的是全校目录
- **不是** `course`（北大教学网 / Blackboard）——那是作业/课件平台，和课程目录无关
