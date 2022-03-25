import {
    clear_folder_recursive,
    compress_to_file,
    extract_to_folder,
} from "./archive";
import { Config, Game, Saves, Save } from "./saveTypes";
import { get_config, set_config } from "./config";
import path from "path";
import moment from "moment";
import fs from "fs";
import { TimeLike } from "original-fs";

// TODO:把同步操作改成异步的以优化性能
// TODO:修改已有的信息

/**
 * 通过游戏名得到游戏存档集合
 * @param game_name 游戏名
 * @returns 当前游戏的存档集合
 */
export function get_game_saves_info(game_name: string) {
    let config = get_config();
    let game_save_path = path.join(config.backup_path, game_name);
    let saves: Saves = (config = JSON.parse(
        fs.readFileSync(path.join(game_save_path, "Saves.json")).toString()
    ));
    return saves;
}

/**
 * 将该游戏原有的存档集合替换成新的
 * @param game_name 游戏名
 * @param new_saves 修改后的存档集合信息
 */
export function set_game_saves_info(game_name: string, new_saves: Saves) {
    let config = get_config();
    let saves_path = config.backup_path;
    fs.writeFileSync(
        path.join(saves_path, game_name, "Saves.json"),
        JSON.stringify(new_saves)
    );
}

/**
 * 通过输入必要的信息来备份指定游戏的存档
 * @param game_name 需要备份的游戏名
 * @param describe 当前存档的描述信息
 */
export function backup_save(
    game_name: string,
    describe: string,
) {
    let config = get_config();
    let game_save_path = config.games[game_name].save_path;
    let backup_path = path.join(config.backup_path, game_name);
    // moment使用参考 http://momentjs.cn/docs/#/displaying/
    let date = moment().format("YYYY-MM-DD_HH-mm-ss");

    let save_info: Save = {
        date: date,
        describe: describe,
        path: path.join(backup_path, date + ".zip"),
    };

    let saves = get_game_saves_info(game_name);
    saves.saves.unshift(save_info);

    set_game_saves_info(game_name, saves);
    compress_to_file(game_save_path, backup_path, date);
}

/**
 * 通过指定游戏名和存档时间来恢复备份
 * @param game_name 游戏名
 * @param save_date 存档时间
 */
export function apply_backup(game_name: string, save_date: TimeLike) {
    let config = get_config();
    let game_save_path = config.games[game_name].save_path;
    let backup_path = path.join(
        config.backup_path,
        game_name,
        save_date + ".zip"
    );
    if(config.settings.extra_backup_when_apply){
        create_extra_backup(game_name)
    }
    extract_to_folder(backup_path, game_save_path);
}

/**
 * 创建额外备份
 * @param game_name 游戏名
 */
function create_extra_backup(game_name:string){
    let config = get_config();
    let game_save_path = config.games[game_name].save_path;
    let extra_backup_path = path.join(config.backup_path, game_name,"extra_backup");

    if(!fs.existsSync(extra_backup_path)){fs.mkdirSync(extra_backup_path)};
    fs.readdir(extra_backup_path,(err,files)=>{
        if(files && files.length>=5){
            files.sort();
            console.log("删除额外备份: ",files[0])
            fs.unlinkSync(path.join(extra_backup_path,files[0]));
        }
    })

    let date = moment().format("被覆盖时间YYYY-MM-DD_HH-mm-ss");
    compress_to_file(game_save_path, extra_backup_path, date);
}

/**
 * 通过必要信息，创建一个游戏的空的存档集合
 * @param game_name 游戏名
 * @param icon 游戏图标
 */
function create_save_folder(game_name: string, icon: string) {
    let config = get_config();
    let saves: Saves = {
        name: game_name,
        saves: [],
        icon: icon,
    };

    if (!fs.existsSync(path.join(config.backup_path))) {
        fs.mkdirSync(path.join(config.backup_path));
    }
    fs.mkdirSync(path.join(config.backup_path, game_name));
    fs.writeFileSync(
        path.join(config.backup_path, game_name, "Saves.json"),
        JSON.stringify(saves)
    );
}

/**
 * 通过必要信息，创建一个游戏的备份库
 * @param game_name 游戏名
 * @param save_path 游戏存档所在位置
 * @param icon 游戏图标
 * @param game_path 游戏启动路径
 */
export function create_game_backup(
    game_name: string,
    save_path: string,
    icon: string,
    game_path: string
) {
    let game: Game = {
        save_path: save_path,
    };
    if (game_path) {
        game.game_path = game_path;
    }

    let config = get_config();
    config.games[game_name] = game;

    create_save_folder(game_name, icon);
    set_config(config);
}
/**
 * 删除某个游戏的单个存档
 * @param game_name 游戏名
 * @param save_date 存档名（同时也是存档日期）
 */
export function delete_save(game_name: string, save_date: string) {
    let config = get_config();
    let save_path = path.join(
        config.backup_path,
        game_name,
        save_date + ".zip"
    );
    console.log("删除单个存档文件", save_path);
    fs.unlinkSync(save_path);

    let saves = get_game_saves_info(game_name);
    let index = saves.saves.findIndex((item) => item.date == save_date);
    if (index == -1) {
        throw "Save is not exists.";
    }
    delete saves.saves[index];
    saves.saves = saves.saves.filter((item) => {
        return item != undefined;
    });

    set_game_saves_info(game_name, saves);
}
/**
 * 删除单个游戏
 * @param game_name 被删除的游戏名
 */
export function delete_game(game_name: string) {
    // 先删除该游戏的存档文件夹
    let config = get_config();
    let backup_path = path.join(config.backup_path, game_name);
    clear_folder_recursive(backup_path);

    // 再删除游戏在配置文件内的信息
    delete config.games[game_name];
    set_config(config);
}
