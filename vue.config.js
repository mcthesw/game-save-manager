module.exports = {
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
