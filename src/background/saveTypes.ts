import { PathLike, TimeLike } from "original-fs";

export interface Game {
    /**
     * 游戏名
     */
    name: string;
    /**
     * 游戏存档路径
     */
    save_path: PathLike;
    /**
     * 游戏图标
     */
    icon: string;
    /**
     * 游戏启动路径
     */
    game_path?: PathLike;
}
export interface Config {
    /**
     * 本软件版本
     */
    version: string;
    /**
     * 本软件管理的存档存放路径
     */
    saves_path: PathLike;
    /**
     * 各个游戏信息
     */
    games: Array<Game>;
}

export interface Save {
    /**
     * 存档的日期时间
     */
    date: TimeLike;
    /**
     * 对当前存档的描述性文本
     */
    describe: string;
    /**
     * 标签(WIP)
     */
    tags: Array<string>;
    /**
     * 当前存档压缩包存放的路径
     */
    path: PathLike;
}
export interface Saves {
    /**
     * 游戏名(判断存档组的唯一标识)
     */
    name: string;
    /**
     * 存档信息
     */
    saves: Array<Save>;
}
