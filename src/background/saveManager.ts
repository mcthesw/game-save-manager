import { compress_to_file, extract_to_folder } from "./archive";
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
    let game_save_path = config.games[game_name].save_path;
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
function set_game_saves_info(game_name: string, new_saves: Saves) {
    let config = get_config();
    let game_save_path = config.games[game_name].save_path;
    fs.writeFileSync(
        path.join(game_save_path, "Saves.json"),
        JSON.stringify(new_saves)
    );
}

/**
 * 通过输入必要的信息来备份指定游戏的存档
 * @param game_name 需要备份的游戏名
 * @param describe 当前存档的描述信息
 * @param tags 当前存档的标签
 */
export function backup_save(
    game_name: string,
    describe: string,
    tags: Array<string>
) {
    let config = get_config();
    let game_save_path = config.games[game_name].save_path;
    let backup_path = path.join(config.backup_path, game_name);
    // moment使用参考 http://momentjs.cn/docs/#/displaying/
    let date = moment().format("YYYY-MM-DD_HH-mm-ss");

    let save_info: Save = {
        date: date,
        describe: describe,
        tags: tags,
        path: path.join(backup_path, date + ".zip"),
    };

    let saves = get_game_saves_info(game_name);
    saves.saves.push(save_info);

    set_game_saves_info(game_name, saves);
    compress_to_file(game_save_path, backup_path, date);
}

/**
 * 通过指定游戏名和存档时间来恢复备份
 * @param game_name 游戏名
 * @param date 存档时间
 */
export function apply_backup(game_name: string, date: TimeLike) {
    let config = get_config();
    let game_save_path = config.games[game_name].save_path;
    let backup_path = path.join(config.backup_path, game_name, date + ".zip");

    extract_to_folder(backup_path, game_save_path);
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

    if(!fs.existsSync(path.join(config.backup_path))){
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
    game_path?: string
) {
    let game: Game = {
        save_path: save_path,
    };
    if (game_path) {
        game.game_path = game_name;
    }

    let config = get_config();
    config.games[game_name] = game;

    create_save_folder(game_name, icon);
    set_config(config);
}
