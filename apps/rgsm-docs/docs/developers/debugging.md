---
title: 调试
---

## 界面与业务流程

```bash
pnpm web:dev
```

启动后打开本地页面，用浏览器开发者工具查看组件、网络请求和控制台。Rust HTTP Host 使用 `.rgsm-dev/app-data` 中的开发数据。

修改 HTTP 接口后，按[接口生成步骤](./architecture.md#修改接口)更新客户端并重新启动开发服务。

## 桌面功能

```bash
pnpm dev
```

托盘、全局快捷键、窗口状态和系统通知通过桌面程序调试。Windows 和 Linux 下可用 `Ctrl+Shift+I` 打开 WebView 开发者工具。

Windows 也可以在 PowerShell 中启用 WebView2 远程调试：

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9222'
pnpm dev
```

从 `http://127.0.0.1:9222/json/list` 获取页面调试入口。关闭程序后，清理当前终端的配置：

```powershell
Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
```

## 文档

```bash
pnpm docs:dev
```

文档站默认在 `http://localhost:3000` 启动，修改页面后自动更新。
