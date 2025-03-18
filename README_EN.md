# Game-save-manager 💖

[![translate](https://hosted.weblate.org/widget/game-save-manager/-/en_US/287x66-grey.png)](https://hosted.weblate.org/engage/game-save-manager)

🌍 [简体中文](README.md) | [English](README_EN.md)

This is a simple and easy-to-use open source game save manager. It can help you manage your game save files, and describe, save, delete, and overwrite your saves in a user-friendly graphical window. The current version supports features such as cloud backup (WebDAV) and quick operations, and considering the performance needs of players, the software has a very small footprint.

- [Official Website](https://help.sworld.club): Provides resources such as help documentation and downloads
- [Changelog](https://help.sworld.club/blog): Recent updates can be viewed on Github and here
- [Milestone](https://github.com/mcthesw/game-save-manager/milestone/3): Records the functions planned to be implemented in the future
- [QQ Group](http://qm.qq.com/cgi-bin/qm/qr?_wv=1027&k=2zkfioUwcqA-Y2ZZqfnhjhQcOUEfcYFD&authKey=7eFKqarle0w7QUsFXZbp%2BLkIvEI0ORoggsnNATOSU6maYiu9mSWSTRxcSorp9eex&noverify=0&group_code=837390423): 837390423

Feature list:

- Delete before restoring (Optional)
- WebDAV supported and a path can be specified
- Can quickly open the save location
- Supports multiple files and folders
- Scheduled backups
- Tray shortcuts

This software uses [Weblate](https://weblate.org/) for translation, and you can participate in the contribution through the icon above

## User Guide 👻
>
>It is recommended to read the guide on the [official website](https://help.sworld.club), this is a simplified version
>
### Download the software 😎

You can download the software from the [download page of the official website](https://help.sworld.club/docs/intro), and you can download the latest test version from the [Release Page](https://github.com/mcthesw/game-save-manager/releases). Users of Win10 or above are recommended to use the portable version. It is worth noting that this software depends on WebView2. If you are not using it on Windows, please install it manually. If you are using Win7 or your system does not come with WebView2, please read the text below carefully.

#### Win7 users please note ⚠️

This software depends on WebView2 to run, and Win7 and some special versions of Windows do not come with this environment, so there are two ways to install the environment

- Use the msi format installation package, which will ask to install the runtime environment if there is a network connection
- Download the runtime environment from the [official website](https://developer.microsoft.com/zh-cn/microsoft-edge/webview2/)

#### Msi installation package users please note ⚠️

This software will install all content to the location specified by the installer, will not create additional folders, and will empty the folder when "Delete application data" is checked during uninstallation. If you installed it in the wrong location, you can refer to [this tutorial](https://help.sworld.club/docs/help/install_to_wrong_location) to solve the problem

### Submit issues | Feature suggestions 😕

You can make suggestions or submit feedback from the following platforms, I will see and reply as soon as possible, but it is best to raise an Issue on Github so that we can resolve it as soon as possible. Of course, you can also participate in the discussion in the QQ group

- 📝[Github Issue](https://github.com/mcthesw/game-save-manager/issues/new/choose)
- 🤝[Github Discussion](https://github.com/mcthesw/game-save-manager/discussions)
- ⚡[Bilibili](https://space.bilibili.com/4087637)

## Developer Guide 🐱
>
>If you are looking for the old developer guide based on the Electron framework, please see the [old branch](https://github.com/mcthesw/game-save-manager/tree/v0-electron)

If you can personally participate in this project, it would be great. Whether it's solving problems or adding new features, we are very welcome. The documentation used by developers will be placed in the `doc/<language>` folder of this repository. Please click [this link](doc/en/README.md) to view

The technologies used in this project:

- Rust
- TypeScript
- Vue3
- Element Plus
- Tauri

## Donate

[![ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/M4M2XD2WO)

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=mcthesw/game-save-manager&type=Date)](https://star-history.com/#mcthesw/game-save-manager&Date)
