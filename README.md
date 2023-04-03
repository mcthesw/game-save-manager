# game-save-manager :heart:
这是一个简单易用的开源游戏存档管理工具，当前版本文档尚不完善，但是软件核心功能已经实现。
它可以帮助你管理游戏的存档文件，并且以用户友好的图像化窗口对你的存档进行描述、保存、删除、覆盖等操作。

[下个版本计划](https://github.com/mcthesw/game-save-manager/milestone/1)
## 普通用户指南:ghost:
#### 启动程序 :sunglasses:
在[ Release 页面](https://github.com/mcthesw/game-save-manager/releases)([国内地址](https://gitee.com/sworldS/game-save-manager/releases/))下载已经打包好的文件，放在你喜欢的地方(最好有着充足的储存空间，且无权限限制)，左键双击其中的EXE文件以启动该软件。

该软件暂时没有制作安装包的计划，因为备份的存档目前是放在软件安装目录下的，如果使用安装包的话有可能会有一些权限的问题需要处理，而且默认安装是放在C盘的，备份多了之后体积可能也是个问题，而现在的重点可能并不在这，之后可能会加入安装包。该软件目前虽然不支持WebDAV，但是可以通过放入OneDrive网盘或坚果云，实现云备份的功能，不过如果在多个设备同步的话请注意备份路径的问题。

#### 问题提交 | 功能建议 :confused:
请点击这里：[Github Issue](https://github.com/mcthesw/game-save-manager/issues/new/choose)
你也可以在小黑盒、[哔哩哔哩](https://space.bilibili.com/4087637)给我提出建议，我会看到会尽快回复的，不过最好还是在 Github 提出 Issue
## 开发者指南:smile_cat:
有能力的话，你也可以亲自参与这个项目，改善这个存档管理器，请 fork 本仓库然后提交 PR，或者提出Issue也是很好的

随着版本的更新，我会逐步完善文档和优化结构，敬请期待
#### 安装依赖:space_invader:
首先安装 pnpm
```
npm i -g pnpm@latest
```
如果您在中国，可以使用镜像加速 electron 的下载，请参考 [.npmrc](.npmrc) 的注释

如果你已经安装，或者安装完成后，可以开始安装依赖
```
pnpm i
```

#### 调试运行:dizzy:
```
pnpm run tauri dev
```

#### 打包构建:robot:
打包指令如下
```
pnpm run tauri build
```
