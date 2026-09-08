---
title: 贡献流程
---

## 提交改动

1. 先创建 issue，说明要解决的问题。小改动可直接在 issue 中说明自己想要提交 PR，大改动必须先与维护者讨论，以确定方向合适
2. Fork 仓库，从最新 `dev` 创建分支
3. 围绕 issue 修改代码，补充相关测试和文档
4. 完成[测试与检查](./testing.md)，检查提交差异
5. 推送分支，向主仓库的 `dev` 提交 PR，并关联 issue

PR 描述说明解决的问题和变化后的行为；界面改动附上截图。

## 提交信息

使用英文[约定式提交](https://www.conventionalcommits.org/)，格式为 `type(scope): :emoji: summary`：

```text
fix(backup): :bug: handle missing snapshot files
docs(guide): :memo: clarify the restore steps
```

将不同目的的改动拆成独立提交。合并以 rebase 为主，小改动也可使用 squash。

## 同步上游

首次配置主仓库地址：

```bash
git remote add upstream https://github.com/mcthesw/game-save-manager.git
```

在自己的功能分支中同步：

```bash
git fetch upstream
git rebase upstream/dev
```

解决冲突后，重新运行相关检查。版本号和发布流程由维护者统一处理。

## 文档与翻译

- [改进文档](../contribute/document.md)：用简短步骤说明当前行为
- [参与翻译](../contribute/translate.md)：通过 Weblate 完善软件翻译
- 中文开发者指南位于 `apps/rgsm-docs/docs/developers`，英文对应页面位于 `i18n/en/docusaurus-plugin-content-docs/current/developers`
