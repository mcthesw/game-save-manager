import { TimeLike } from "original-fs";

export interface Game {
    /**
     * 游戏存档路径
     */
    save_path: string;
    /**
     * 游戏启动路径
     */
    game_path?: string;
}
export interface Games {
    /**
     * 使用游戏名来找到游戏信息
     */
    [name: string]: Game;
}
export interface Config {
    /**
     * 本软件版本
     */
    version: string;
    /**
     * 本软件管理的存档存放路径
     */
    backup_path: string;
    /**
     * 各个游戏信息
     */
    games: Games;
    /**
     * 存档管理器的配置信息
     */
    settings:Settings;
}

export interface Save {
    /**
     * 存档的日期时间(和Saves中的游戏名可确定唯一存档)
     */
    date: TimeLike;
    /**
     * 对当前存档的描述性文本
     */
    describe: string;
    /**
     * 当前存档压缩包存放的路径
     */
    path: string;
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
    /**
     * 游戏图标,可以是base64也可以是url
     */
    icon: string;
}

export interface Settings {
    /**
     * 是否允许不输入描述就存档
     */
    prompt_when_not_described: boolean;
    /**
     * 是否在应用存档时进行额外备份
     */
    extra_backup_when_apply:boolean;
}
