import { ipcMain } from "electron";
import fs from "fs";
import { Config } from "./saveTypes";

export let default_config: Config = {
    version: "0.3.0",
    backup_path: "./save_data",
    games: {},
    settings:{
        prompt_when_not_described:false,
        extra_backup_when_apply:true,
    }
};

function init_config() {
    // 初始化配置文件
    console.log("初始化配置文件");

    fs.writeFileSync(
        "./GameSaveManager.config.json",
        JSON.stringify(default_config)
    );
}

export function get_config() {
    let config: Config;
    config = JSON.parse(
        fs.readFileSync("./GameSaveManager.config.json").toString()
    );
    return config;
}

export function set_config(config: Config) {
    fs.writeFileSync("./GameSaveManager.config.json", JSON.stringify(config));
}

export function config_check() {
    // 每次程序启动时的检查
    // 存档是否存在
    if (!fs.existsSync("./GameSaveManager.config.json")) {
        init_config();
    }
    let config = get_config();

    // 检查版本
    if(config.version == "0.3.0"){
        console.log("存档版本正确")
    }
    else if (config.version == "0.2.0") {
        console.log("检测到0.2.0老版本config");
        config.version = default_config.version;
        config.settings.extra_backup_when_apply = true;
        console.log("执行升级config版本")
        set_config(config);
    }else{
        ipcMain.emit("unknown_config_version",true);
    }
}
