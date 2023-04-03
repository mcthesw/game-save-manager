module.exports = {
  pluginOptions: {
    electronBuilder: {
      nodeIntegration: true,
      builderOptions: {
        productName: "GameSaveManager",
        win:{
          target:"zip",
          //icon:"./public/icon.png"
        },
        linux:{
          target:"zip"
        },
        mac:{
          target:"zip"
        }
      },
    },
  },
};
