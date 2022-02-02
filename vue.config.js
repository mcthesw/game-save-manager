module.exports = {
  pluginOptions: {
    electronBuilder: {
      nodeIntegration: true,
      builderOptions: {
        productName: "游戏存档管理",
        nsis: {
          oneClick: false, // 是否一键安装
          allowElevation: true, // 允许请求提升。 如果为false，则用户必须使用提升的权限重新启动安装程序。
          allowToChangeInstallationDirectory: true, // 允许修改安装目录
          // "installerIcon": "./public/favicon.ico",// 安装图标
          // "uninstallerIcon": "./public/favicon.ico",//卸载图标
          // "installerHeaderIcon": "./public/favicon.ico", // 安装时头部图标
          createDesktopShortcut: true, // 创建桌面图标
          createStartMenuShortcut: true, // 创建开始菜单图标
          shortcutName: "游戏存档管理", // 图标名称
        },
      },
    },
  },
};
