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

注意：发布命令是顶层的 `clawhub publish <path>`，不是 `clawhub skill publish`
（`clawhub skill` 下只有 `rename` / `merge` 等管理命令）。

### 首次发布所有 skills

```bash
clawhub publish ./skills/treehole   --slug pku-treehole   --name "PKU Treehole"     --version 1.0.0 --tags "pku,treehole,cli,rust"
clawhub publish ./skills/course     --slug pku-course     --name "PKU Course"       --version 1.0.0 --tags "pku,blackboard,cli,rust"
clawhub publish ./skills/campuscard --slug pku-campuscard --name "PKU Campus Card"  --version 1.0.0 --tags "pku,campus-card,cli,rust"
clawhub publish ./skills/elective   --slug pku-elective   --name "PKU Elective"     --version 1.0.0 --tags "pku,elective,cli,rust"
clawhub publish ./skills/info-spider --slug pku-info-spider --name "PKU Info Spider" --version 1.0.0 --tags "pku,wechat,crawler,rust"
clawhub publish ./skills/info-common --slug pku-info-common --name "PKU Info Common" --version 1.0.0 --tags "pku,iaaa,auth,rust"
clawhub publish ./skills/info-auth  --slug pku-info-auth  --name "PKU Info Auth"    --version 1.0.0 --tags "pku,auth,keyring,credential,cli,rust"
clawhub publish ./skills/bdkj       --slug pku-bdkj       --name "PKU BDKJ"         --version 1.0.0 --tags "pku,bdkj,classroom,reservation,cli,rust"
clawhub publish ./skills/cwfw       --slug pku-cwfw       --name "PKU CWFW"         --version 1.0.0 --tags "pku,cwfw,finance,payroll,cli,rust"
clawhub publish ./skills/portal     --slug pku-portal     --name "PKU Portal"       --version 1.0.0 --tags "pku,portal,classroom,calendar,netfee,wechat,cli,rust"
```

### 版本更新

`--version` **必须是显式 semver**，不支持 `patch`/`minor`/`major` 别名。
先用 `clawhub inspect <slug>` 查看当前 `Latest:` 行，再手动递增：

```bash
clawhub inspect pku-treehole        # 比如看到 Latest: 1.1.0
clawhub publish ./skills/treehole --slug pku-treehole --version 1.2.0 \
  --changelog "具体变更说明"
```

`--name` 和 `--tags` 在首次发布后可省略，重新指定会覆盖老值。

### 批量同步

```bash
clawhub sync
```

自动扫描本地 `skills/` 目录，对新增/有变更的 skill 发起发布。对已有 skill 仍需要先
保证 `--version` 已在本地某处递增，否则 registry 会拒绝重复版本。
