import fs from "fs";

function init_config() {
  // 初始化配置文件
  console.log("初始化配置文件");
  let default_config = JSON.stringify({
    version: "0.0.1",
    saves_path: "./save_data",
    games:[]
  });
  fs.writeFileSync("./GameSaveManager.config.json", default_config);
}

export function get_config() {
  let config:any;
  config = JSON.parse(fs.readFileSync("./GameSaveManager.config.json").toString())
  return config
}

export function config_check() {
  // 每次程序启动时的检查
  if (!fs.existsSync("./GameSaveManager.config.json")) {
    init_config();
  }
}