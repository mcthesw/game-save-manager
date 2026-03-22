# Game-save-manager 💖

[![translate](https://hosted.weblate.org/widget/game-save-manager/-/en_US/287x66-grey.png)](https://hosted.weblate.org/engage/game-save-manager)

🌍 [简体中文](README.md) | [English](README_EN.md)

这是一个简单易用的开源游戏存档管理工具。它可以帮助你管理游戏的存档文件，并且以用户友好的图像化窗口对你的存档进行描述、保存、删除、覆盖等操作。当前版本已经支持了云备份（WebDAV）、快捷操作等功能，且考虑到玩家的性能需求，该软件后台占用极小。

- [官方网站](https://help.sworld.club)：提供了帮助文档、下载等资源
- [更新日志](https://help.sworld.club/blog)：Github和这里都可以看到最近的更新
- [下个版本计划](https://github.com/mcthesw/game-save-manager/milestone/3)：记录了未来计划实现的功能
- [路线图](ROADMAP.md)：V2.0 及未来版本的开发方向和架构决策
- [QQ交流群](http://qm.qq.com/cgi-bin/qm/qr?_wv=1027&k=2zkfioUwcqA-Y2ZZqfnhjhQcOUEfcYFD&authKey=7eFKqarle0w7QUsFXZbp%2BLkIvEI0ORoggsnNATOSU6maYiu9mSWSTRxcSorp9eex&noverify=0&group_code=837390423)：837390423

特性列表：

- 可以在恢复前删除
- 可以云备份，并指定云路径
- 可以快速打开存挡位置
- 支持多文件、文件夹
- 定时备份
- 角标快捷操作

该软件使用[Weblate](https://weblate.org/)进行翻译，你可以通过上方图标参与贡献

## 用户使用指南 👻

> 建议阅读[官方网站](https://help.sworld.club)的指南，此处为简略版

### 下载软件 😎

在[官网的下载界面](https://help.sworld.club/docs/intro)可以下载到本软件，而在[Release 页面](https://github.com/mcthesw/game-save-manager/releases)可以下载到前沿测试版。Win10 或以上版本的用户推荐使用便携版(portable)，值得注意的是，本软件依赖于 WebView2 ，如果你不在 Win 上使用，请手动安装，如果你使用的是 Win7，或者你的系统没有附带 WebView2，请注意阅读下方文本。

#### Win7用户请注意 ⚠️

本软件依赖 WebView2 运行，而 Win7 和一些特殊版本的 Windows 并没有自带此环境，因此提供两个办法安装环境

- 使用 msi 格式的安装包，在有网络的情况下它会要求安装运行环境
- 从[官方网站](https://developer.microsoft.com/zh-cn/microsoft-edge/webview2/)下载安装运行环境

#### 安装包用户请注意 ⚠️

本软件会将全部内容安装到安装程序指定的位置，不会额外创建文件夹，且在卸载时勾选“删除应用程序数据”会清空该文件夹，如果你安装到了错误的位置，可以参考[这篇教程](https://help.sworld.club/docs/help/install_to_wrong_location)解决

### 问题提交 | 功能建议 😕

你可以从以下平台提出建议或反馈问题，我会看到会尽快回复的，不过最好还是在 Github 提出 Issue，以便我们尽快解决，当然，也可以在QQ群参与讨论

- 📝[Github Issue](https://github.com/mcthesw/game-save-manager/issues/new/choose)
- 🤝[Github Discussion](https://github.com/mcthesw/game-save-manager/discussions)
- ⚡[哔哩哔哩](https://space.bilibili.com/4087637)

## 开发者指南 🐱

> 如果你在寻找旧版基于Electron框架的开发者指南，请看[旧版分支](https://github.com/mcthesw/game-save-manager/tree/v0-electron)

如果你能亲自参与这个项目，那真的太好了，不论是解决问题，还是说添加新功能，我们都是非常欢迎的，开发者使用的文档将会放置于本仓库的 `doc/<language>` 文件夹下，请点[此链接](doc/zh-CN/README.md)查看

本项目使用的技术：

- Rust
- TypeScript
- Vue3
- Element Plus
- Tauri

## 捐赠

你可以通过[爱发电](https://afdian.net/a/Sworld)和微信支持这个项目，可以参考[这个网页](https://help.sworld.club/docs/about)

## 致谢

- [Linux DO](https://linux.do) 真诚、友善、团结、专业，共建你我引以为荣之社区。
- [Weblate](https://weblate.org) 专业的翻译网站，为本项目提供了翻译社区支持。

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=mcthesw/game-save-manager&type=Date)](https://star-history.com/#mcthesw/game-save-manager&Date)
