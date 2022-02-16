import { ipcMain, shell, dialog, nativeImage } from "electron";
import { get_config } from "./config";

export function init_ipc() {
  ipcMain.on("open_url", async (Event, arg) => {
    // 打开URL
    shell.openExternal(arg);
  });
  ipcMain.on("choose_save_directory", async (Event, arg) => {
    // 选择游戏存档目录
    const path = dialog.showOpenDialog({
      title: "请选择存档路径",
      properties: ["openDirectory"],
    });
    Event.reply("choose_save_directory_reply", await path);
  });

  ipcMain.on("choose_executable_file", async (Event, arg) => {
    // 选择游戏可执行文件
    const path = dialog.showOpenDialog({
      title: "选择游戏可执行文件",
      properties: ["openFile"],
      filters: [
        { name: "可执行程序", extensions: ["exe", "bat", "cmd", "jar"] },
      ],
    });
    Event.reply("choose_executable_file_reply", await path);
  });

  ipcMain.on("choose_game_icon", async (Event, arg) => {
    // 选择游戏图标
    const path = dialog.showOpenDialog({
      title: "选择游戏图标",
      properties: ["openFile"],
      filters: [{ name: "可识别图片", extensions: ["jpg", "png", "ico"] }],
    });
    const icon = nativeImage.createFromPath((await path).filePaths[0]);
    if (icon == undefined) {
      return;
    }
    Event.reply("choose_game_icon_reply", icon.toDataURL());
  });

  ipcMain.on("get_config", async (Event, arg) => {
    // 返回本地的配置文件
    let config = get_config();
    Event.reply("reply_config", config);
  });
}
