---
title: 项目结构
---

项目使用 pnpm 和 Cargo 工作区。Vue 3、TypeScript 和 Vite 负责前端，Tauri 2 提供桌面窗口和系统集成，Rust 核心库负责存档业务。

| 路径                      | 用途                                 |
| ------------------------- | ------------------------------------ |
| `apps/rgsm-gui/src`       | 页面、组件、组合式函数和 HTTP 客户端 |
| `apps/rgsm-gui/src-tauri` | 桌面宿主、HTTP 路由、托盘和快捷键    |
| `crates/rgsm-core`        | 备份、恢复、配置、云同步和应用服务   |
| `apps/rgsm-docs`          | 用户文档和开发者指南                 |
| `locales`                 | 前后端共用的翻译                     |
| `scripts`                 | 仓库级构建和打包工具                 |
| `docs`                    | 设计说明和架构决策记录               |
| `CONTEXT.md`              | 当前领域术语和约定                   |

CLI、TUI 和 FFI 的入口分别位于 `apps/rgsm-cli`、`apps/rgsm-tui` 和 `crates/rgsm-ffi`。

## 一次界面操作的调用路径

Vue 页面 → TypeScript HTTP 客户端 → Rust 路由 → 应用服务 → 领域模块

前端通过 HTTP 请求调用业务，使用 SSE 接收事件。桌面和浏览器调试共用这套接口。

- 页面与组件放在 `src/pages`、`src/components`，共享界面逻辑放在 `src/composables`
- 前端使用 Tailwind CSS、Reka UI 和项目内的 UI 组件
- `rgsm-core/src/services` 装配配置、依赖和生命周期 hook
- `backup`、`config`、`cloud_sync` 等领域模块接收显式参数，处理各自的业务规则

## 修改接口

更新 Rust HTTP 合约后，在仓库根目录重新生成客户端：

```bash
pnpm --dir apps/rgsm-gui web:generate-api
```

将生成的 `src/api/generated` 和 `src/api/defaultConfig.generated.ts` 与接口改动一起提交。

## 界面约定

- Vue 组件使用 `<script setup lang="ts">`
- 通知使用 `useNotification()`，确认和输入使用 `useFeedback()`
- 弹层层级使用 `src/ui/layers.ts` 中的 `LAYER`
- 用户可见文字使用翻译键，同步更新 `en_US` 和 `zh_SIMPLIFIED`

领域概念见 [CONTEXT.md](https://github.com/mcthesw/game-save-manager/blob/dev/CONTEXT.md)，设计依据见 [架构决策记录](https://github.com/mcthesw/game-save-manager/tree/dev/docs/adr)。
