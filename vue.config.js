/*
 * @Author: PlanC
 * @Date: 2022-09-28 12:13:50
 * @LastEditTime: 2022-09-28 12:50:30
 * @FilePath: \electron-TRPG-makerd:\Projects\game-save-manager\vue.config.js
 */

module.exports = {
	transpileDependencies: [],
  pluginOptions: {
    electronBuilder: {
      nodeIntegration: true,
      builderOptions: {
        productName: "游戏存档管理",
        win:{
          target:"zip",
          //icon:"./public/icon.png"
        }
      },
    },
  },
};
