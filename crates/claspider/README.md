# pku-claspider

北京大学课程信息爬取工具——拼合教务部课程信息网与选课网的数据，生成可供离线查询的课程目录（开课单位 / 学分 / 教师 / 时间地点 / 简介）。

## 安装

```bash
cargo install pku-claspider
```

或作为 `pku-cli` 的一部分：

```bash
cargo install pku-cli
pku claspider --help
```

## 使用

```bash
# 登录（IAAA 凭据从 keyring/env 读取）
claspider login -p

# 抓取某个学期的全部课程信息
claspider fetch --term 2526fall --out ./courses.json
```

具体子命令见 `claspider --help`。所有持久化文件位于 `~/.config/info/claspider/`。

## 许可

MIT
