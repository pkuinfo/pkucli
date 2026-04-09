# Skills 发布指南

## Claude Code 官方插件市场

Repo 推到 GitHub 后，用户安装：

```bash
/plugin marketplace add wjsoj/info
/plugin install pku-cli@pku-cli
```

更新插件版本：修改 `.claude-plugin/plugin.json` 中的 `version` 字段，推送即可。

## ClawHub (OpenClaw)

### 安装与登录

```bash
npm i -g clawhub
clawhub login
clawhub whoami
```

需要注册满一周的 GitHub 账号。

### 发布所有 skills

```bash
clawhub skill publish ./skills/treehole --slug pku-treehole --name "PKU Treehole" --version 1.0.0 --tags "pku,treehole,cli,rust"
clawhub skill publish ./skills/course --slug pku-course --name "PKU Course" --version 1.0.0 --tags "pku,blackboard,cli,rust"
clawhub skill publish ./skills/campuscard --slug pku-campuscard --name "PKU Campus Card" --version 1.0.0 --tags "pku,campus-card,cli,rust"
clawhub skill publish ./skills/elective --slug pku-elective --name "PKU Elective" --version 1.0.0 --tags "pku,elective,cli,rust"
clawhub skill publish ./skills/info-spider --slug pku-info-spider --name "PKU Info Spider" --version 1.0.0 --tags "pku,wechat,crawler,rust"
clawhub skill publish ./skills/info-common --slug pku-info-common --name "PKU Info Common" --version 1.0.0 --tags "pku,iaaa,auth,rust"
clawhub skill publish ./skills/info-auth --slug pku-info-auth --name "PKU Info Auth" --version 1.0.0 --tags "pku,auth,keyring,credential,cli,rust"
```

### 版本更新

```bash
clawhub skill publish ./skills/<name> --slug pku-<name> --version patch
```

`--version` 支持 `patch` / `minor` / `major` 自动递增，也可指定具体版本号。

### 批量同步

```bash
clawhub sync
```

自动扫描本地目录，发布新增或有变更的 skill。
