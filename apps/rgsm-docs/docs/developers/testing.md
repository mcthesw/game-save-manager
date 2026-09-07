---
title: 测试
---

在仓库根目录运行以下命令。修改功能时，先运行相关测试，再完成提交前检查。

## Rust

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo check --workspace
cargo test -p rgsm-core --lib
```

配置升级和云同步还提供集成测试：

```bash
cargo test -p rgsm-core --test config_upgrade_compatibility
cargo test -p rgsm-core --test cloud_sync_v2_fs
```

## 前端

```bash
pnpm web:format
pnpm web:lint
pnpm web:typecheck
pnpm web:test
```

格式修复使用 `pnpm web:format:fix`，然后检查差异。

## 端到端测试

首次运行前安装 Chromium：

```bash
pnpm --dir apps/rgsm-gui exec playwright install chromium
pnpm web:e2e local-main-path.spec.ts
```

测试使用真实 Rust HTTP Host、临时存档和本地 Fs 云目录。运行前释放 `5173` 端口。

`pnpm web:e2e` 运行完整浏览器套件，`pnpm web:e2e:desktop` 运行桌面场景。平台依赖和场景列表见 [E2E 说明](https://github.com/mcthesw/game-save-manager/blob/dev/apps/rgsm-gui/e2e/README.md)。

## 文档

```bash
pnpm docs:build
```

构建会检查文档链接。提交后在 PR 页面查看 CI 结果。
