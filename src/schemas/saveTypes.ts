import type { Backend } from "./BackendTypes";

export interface SaveUnit {
    unit_type: "File" | "Folder";
    path: string;
    delete_before_apply: boolean;
}
export interface Game {
    name: string,
    /**
     * 游戏存档路径
     */
    save_paths: Array<SaveUnit>;
    /**
     * 游戏启动路径
     */
    game_path?: string;
}

export interface Backup {
    /**
     * 存档的日期时间(和Saves中的游戏名可确定唯一存档)
     */
    date: string;
    /**
     * 对当前存档的描述性文本
     */
    describe: string;
    /**
     * 当前存档压缩包存放的路径
     */
    path: string;
}
export interface BackupsInfo {
    /**
     * 游戏名(判断存档组的唯一标识)
     */
    name: string;
    /**
     * 存档信息
     */
    backups: Array<Backup>;
}

export interface CloudSettings {
    /**
     * 是否启用随时同步
     */
    always_sync: boolean;
    /**
     * 自动同步间隔，单位为分钟，为0则不自动同步
     */
    auto_sync_interval: number;
    /**
     * 云存储根路径
     */
    root_path: string;
    /**
     * 同步的后端设置
     */
    backend: Backend;
}

export interface Settings {
    /**
     * 是否允许不输入描述就存档
     */
    prompt_when_not_described: boolean;
    /**
     * 是否在应用存档时进行额外备份
     */
    extra_backup_when_apply: boolean;
    /**
     * 是否在自动备份时提示
     */
    prompt_when_auto_backup: boolean;
    /**
     * 是否显示"修改存档管理"按钮
     */
    show_edit_button: boolean;
    /**
     * 是否在关闭时最小化到托盘
     */
    exit_to_tray: boolean;
    /**
     * 云存储设置
     */
    cloud_settings: CloudSettings;
    /**
     * 语言配置
     */
    locale: string,
    /**
     * 是否默认先删除存档再备份
     */
    default_delete_before_apply:boolean,
    /**
     * 是否默认展开收藏夹树
     */
    default_expend_favorites_tree:boolean
}

export interface FavoriteTreeNode {
    /**
     * 唯一标识符，使用GUID生成
     */
    node_id: string;
    /**
     * 树节点名称，如果是叶子节点，需要与`Game`对应
     */
    label: string;
    /**
     * 是否为叶子节点，如果是游戏需要为true，是文件夹则为false
     */
    is_leaf: boolean;
    /**
     * 如果是文件夹，则包含子节点，否则为null
     */
    children?: FavoriteTreeNode[];
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
    games: Array<Game>;
    /**
     * 存档管理器的配置信息
     */
    settings: Settings;
    /**
     * 收藏夹
     */
    favorites: Array<FavoriteTreeNode>;
}

export let default_config: Config = {
    version: "1.2.0",
    backup_path: "./save_data",
    games: [],
    settings: {
        prompt_when_not_described: false,
        extra_backup_when_apply: true,
        exit_to_tray: true,
        show_edit_button: false,
        cloud_settings: {
            always_sync: false,
            auto_sync_interval: 0,
            root_path: "/game-save-manager",
            backend: {
                type: "Disabled",
            }
        },
        prompt_when_auto_backup: false,
        locale: "zh_SIMPLIFIED",
        default_delete_before_apply: false,
        default_expend_favorites_tree: false
    },
    favorites: [],
};

export { Backend };
