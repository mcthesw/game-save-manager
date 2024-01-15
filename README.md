# game-save-manager :heart:
这是一个简单易用的开源游戏存档管理工具，当前版本文档尚不完善，但是软件核心功能已经实现。
它可以帮助你管理游戏的存档文件，并且以用户友好的图像化窗口对你的存档进行描述、保存、删除、覆盖等操作。

[下个版本计划](https://github.com/mcthesw/game-save-manager/milestone/1)
## 普通用户指南:ghost:
### 启动程序 :sunglasses:
在[ Release 页面](https://github.com/mcthesw/game-save-manager/releases)([国内地址](https://gitee.com/sworldS/game-save-manager/releases/))可以下载到本软件，推荐使用安装包，Win10 或以上版本的用户也推荐使用便携版(portable)，值得注意的是，本软件依赖于 WebView2 ，如果你不在 Win 上使用，请手动安装，如果你使用的是 Win7，请注意阅读下方文本。

该软件目前虽然不支持WebDAV，但是可以通过放入OneDrive网盘或坚果云，实现云备份的功能，不过如果在多个设备同步的话请注意备份路径的问题。以后预期在新版本或通过插件支持真正的多设备同步存档。

#### Win7用户请注意
由于本软件依赖 WebView2 运行，而 Win7 并没有自带此环境，因此提供两个办法安装环境
- 使用 msi 格式的安装包，在有网络的情况下它会要求安装运行环境
- 从[官方网站](https://developer.microsoft.com/zh-cn/microsoft-edge/webview2/)下载安装运行环境


### 问题提交 | 功能建议 :confused:
请点击这里：[Github Issue](https://github.com/mcthesw/game-save-manager/issues/new/choose)
，你也可以在小黑盒、[哔哩哔哩](https://space.bilibili.com/4087637)给我提出建议，我会看到会尽快回复的，不过最好还是在 Github 提出 Issue，这里还有一个官方 QQ 群 [837390423](http://qm.qq.com/cgi-bin/qm/qr?_wv=1027&k=2zkfioUwcqA-Y2ZZqfnhjhQcOUEfcYFD&authKey=7eFKqarle0w7QUsFXZbp%2BLkIvEI0ORoggsnNATOSU6maYiu9mSWSTRxcSorp9eex&noverify=0&group_code=837390423)
## 开发者指南:smile_cat:
有能力的话，你也可以亲自参与这个项目，改善这个存档管理器，请 fork 本仓库然后提交 PR，或者提出Issue也是很好的，请按照[约定式提交](https://www.conventionalcommits.org/zh-hans/v1.0.0/)来编写 commit 信息，这样有助于合作以及自动化构建

随着版本的更新，我会逐步完善文档和优化结构，敬请期待

当前，新版(V1.0.0 alpha)的文档正在编辑中，如果你在寻找旧版基于Electron框架的开发者指南，请看[旧版分支](https://github.com/mcthesw/game-save-manager/tree/v0-electron)