<script lang="ts" setup>
// !为了不让 submit_handler 报错，这里不使用 lang="ts"
import { DocumentAdd } from "@element-plus/icons-vue";
import {
    Check,
    RefreshRight,
    Download,
} from "@element-plus/icons-vue";
import { reactive, ref } from "vue";
import { useRouter } from "vue-router";
import { useConfig } from "../stores/ConfigFile";
import { invoke } from '@tauri-apps/api/tauri'
import { Game, SaveUnit } from "../schemas/saveTypes";
import { show_error, show_warning } from "../utils/notifications";
import { show_success } from "../utils/notifications";
import { watchEffect, watch } from "vue";
import { useRoute } from "vue-router";
import { $t } from "../i18n";
import { v4 as uuidv4 } from 'uuid';

const route = useRoute();
const router = useRouter();
let config = useConfig();
const buttons = [
    {
        text: $t('addgame.search_local'),
        type: "primary",
        icon: Download,
        method: search_local,
    },
    {
        text: $t('addgame.save_current_profile'),
        type: "success",
        icon: Check,
        method: save,
    },
    {
        text: $t('addgame.reset_current_profile'),
        type: "danger",
        icon: RefreshRight,
        method: reset_info,
    },
] as const;


const game_name = ref("") // 写入游戏名
let save_paths: Array<SaveUnit> = reactive(new Array<SaveUnit>()) // 选择游戏存档目录
const game_path = ref("") // 选择游戏启动程序
const game_icon_src = ref("orange.png")
const is_editing = ref(false) // 是否正在编辑已有的游戏

// init info when navigate from GameManage.vue
watchEffect(() => {
    const gameName = route.params.name;
    if (gameName) {
        const gameConfig = config.games.find(game => game.name === gameName);
        if (gameConfig) {
            is_editing.value = true;
            game_name.value = gameConfig.name;
            save_paths = gameConfig.save_paths;
            game_path.value = gameConfig.game_path || '';
        } else {
            show_error($t('addgame.change_target_not_exists_error') + gameName);
            router.back();
        }
    }
});

function check_save_unit_unique(p: string) {
    if (save_paths.find((x) => x.path == p)) {
        show_error($t('addgame.duplicated_filename_error'));
        return false;
    }
    return true;
}
function check_name_valid(name: string) {
    let invalid_reg = RegExp(/[<>:"\/\\|?*]/);
    return !invalid_reg.test(name);
}
function generate_save_unit(unit_type: "Folder" | "File", path: string): SaveUnit {
    let delete_before_apply = config.settings.default_delete_before_apply;
    return { unit_type, path, delete_before_apply }
}

function add_save_directory() {
    invoke("choose_save_dir").then((dir) => {
        if (!dir || !check_save_unit_unique(dir as string)) { return }
        save_paths.push(
            generate_save_unit("Folder", dir as string)
        )
    }).catch(
        (e) => {
            console.log(e)
            show_error($t('error.choose_save_dir_error'));
        }
    )
}
function add_save_file() {
    invoke("choose_save_file").then((file) => {
        if (!file || !check_save_unit_unique(file as string)) { return }
        save_paths.push(
            generate_save_unit("File", file as string)
        )
    }).catch(
        (e) => {
            console.log(e)
            show_error($t('error.choose_save_file_error'));
        }
    )
}
function choose_executable_file() {
    invoke("choose_save_file").then((file) => {
        console.log(file);
        game_path.value = file as string;
    }).catch(
        (e) => {
            console.log(e)
            show_error($t('error.choose_executable_file_error'));
        }
    )
}

function submit_handler(button_method: Function) {
    // 映射按钮的ID和他们要触发的方法
    button_method();
}
function search_local() {
    // TODO:导入已有配置
    show_warning($t('addgame.wip_warning'));
}
async function save() {
    // 去除头尾空字符，防止触发Windows文件命名规则问题
    game_name.value = game_name.value.trim();
    if (game_name.value == "" || save_paths.length == 0) {
        show_error($t('addgame.no_name_error'));
        return;
    }
    if (!check_name_valid(game_name.value)) {
        show_error($t('addgame.invalid_name_error'));
        return;
    }
    if (config.games.find((x) => x.name.toLowerCase() == game_name.value.toLowerCase())) {
        show_error($t('addgame.duplicated_name_error'));
        return;
    }

    let game: Game = {
        name: game_name.value,
        save_paths: save_paths,
        game_path: game_path.value
    };
    try {
        const result = await invoke("add_game", { game: game });
        console.log(result);

        if (is_editing.value) {
            is_editing.value = false;
            show_success($t('addgame.add_game_success'));
            router.back();
        } else {
            if (config.settings.add_new_to_favorites) {
                // TODO:以下内容是否需要抽离成单独的工具库？还是说应该后端处理？
                await config.refresh();
                config.favorites?.push({
                    label: game.name,
                    is_leaf: true,
                    children: [],
                    node_id: uuidv4().toString()
                });
                await config.save();
            }
            show_success($t('addgame.add_game_success'));
        }
        reset_info(false);
        await config.refresh();
    } catch (e) {
        console.log(e);
        show_error($t('error.add_game_failed'));
    }
}
function reset_info(show_notification: boolean = true) {
    // 重置当前配置
    game_name.value = "";
    save_paths = reactive([]);
    game_path.value = "";
    // TODO:This is a first occurrence of a i18n text duplication. How to handle this?
    if (show_notification) { show_success($t('settings.reset_success')); }
}

function deleteRow(index: number) {
    save_paths.splice(index, 1);
}
</script>

<template>
    <div class="select-container">
        <el-card class="game-info">
            <div class="top-part">
                <img class="game-icon" :src="game_icon_src" />
                <div class="bold">
                    {{ $t("addgame.warning_for_save_file") }}
                </div>
                <el-input v-model="game_name" :placeholder="$t('addgame.input_game_name_prompt')" class="game-name">
                    <template #prepend>
                        {{ $t('addgame.game_name') }} </template>
                </el-input>
                <el-input v-model="game_path" :placeholder="$t('addgame.input_game_launch_path_prompt')"
                    class="game-path">
                    <template #prepend>
                        {{ $t('addgame.game_launch_path') }} </template>
                    <template #append>
                        <el-button @click="choose_executable_file()">
                            <el-icon>
                                <document-add />
                            </el-icon>
                        </el-button>
                    </template>
                </el-input>
            </div>
            <div class="add-button-area">
                <el-button type="primary" @click="add_save_directory">{{ $t('addgame.add_save_directory') }}</el-button>
                <el-button type="primary" @click="add_save_file">{{ $t('addgame.add_save_file') }}</el-button>
            </div>
            <el-table :data="save_paths" class="save-table">
                <el-table-column fixed prop="unit_type" :label="$t('addgame.type')" width="120" />
                <el-table-column :label="$t('addgame.operations')" width="120">
                    <template #default="scope">
                        <el-button link type="primary" size="small" @click.prevent="deleteRow(scope.$index)">
                            {{ $t('addgame.remove') }} </el-button>
                    </template>
                </el-table-column>
                <el-table-column prop="path" :label="$t('addgame.path')" />
            </el-table>
        </el-card>
        <el-container class="submit-bar">
            <el-tooltip v-for="button in buttons" :key="button.text" :content="button.text" placement="top">
                <el-button @click="submit_handler(button.method)" :type="button.type" circle>
                    <el-icon>
                        <component :is="button.icon" />
                    </el-icon>
                </el-button>
            </el-tooltip>
        </el-container>
    </div>
</template>

<style scoped>
.bold {
    margin-left: 10px;
    font-weight: bold;
    color: var(--el-text-color-primary);
}

.save-table {
    margin-top: 20px;
    margin-bottom: 20px;
}

.select-container {
    height: 90%;
    width: 100%;
}

.el-card {
    margin-bottom: 15px;
    padding-bottom: 20px;
}

.top-part {
    height: 200px;
    display: grid;
    grid-template-columns: 1fr 3fr;
    grid-template-rows: 1fr 1fr 1fr 1fr 1fr 1fr;
}

.top-part>img {
    grid-column: 1/2;
    grid-row: 1/7;
    margin: auto;
}

.game-name {
    grid-column: 2/3;
    grid-row: 5/6;
    margin-bottom: 5px;
}

.game-path {
    grid-column: 2/3;
    grid-row: 6/7;
}

.game-icon {
    float: left;
    height: 200px;
    width: 200px;
}

.add-button-area {
    margin-top: 20px;
}

.submit-bar {
    justify-content: flex-end;
    height: 10%;
}
</style>