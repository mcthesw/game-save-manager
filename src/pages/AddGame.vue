<script lang="ts" setup>
import {
    DocumentAdd,
    Check,
    RefreshRight,
    Download,
    QuestionFilled,
} from "@element-plus/icons-vue";
import { reactive, ref, watchEffect } from "vue";
import { commands, type Game, type SaveUnit, type Device } from "../bindings";
import { $t } from "../i18n";
import { v4 as uuidv4 } from 'uuid';
import { error } from "@tauri-apps/plugin-log";
import PathVariableSelector from "../components/PathVariableSelector.vue";

const route = useRoute();
const router = useRouter();
const { showError, showWarning, showSuccess } = useNotification();
const { config, refreshConfig, saveConfig } = useConfig();
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
const game_icon_src = ref("/orange.png")
const is_editing = ref(false) // 是否正在编辑已有的游戏
const currentDevice = ref<Device | null>(null) // 当前设备信息

// 获取当前设备信息
async function fetchCurrentDevice() {
    try {
        const result = await commands.getCurrentDeviceInfo();
        if (result.status === "ok") {
            currentDevice.value = result.data;
            console.log("Current device:", currentDevice.value);
        } else {
            showError({ message: result.error });
        }
    } catch (e) {
        error(`Error getting current device info: ${e}`);
        showError({ message: $t('error.get_device_info_failed') });
    }
}
// 在组件挂载时获取当前设备信息
fetchCurrentDevice();

// init info when navigate from GameManage.vue
watchEffect(() => {
    const gameName = route.params.name;
    if (gameName) {
        const gameConfig = config.value?.games.find(game => game.name === gameName);
        if (gameConfig) {
            is_editing.value = true;
            game_name.value = gameConfig.name;
            save_paths = gameConfig.save_paths;
            
            // 获取当前设备的游戏路径
            if (gameConfig.game_paths && currentDevice.value) {
                const deviceId = currentDevice.value.id;
                game_path.value = gameConfig.game_paths[deviceId] || '';
            } else {
                game_path.value = '';
            }
        } else {
            showError({ message: $t('addgame.change_target_not_exists_error') + gameName });
            router.back();
        }
    }
});

function check_save_unit_unique(p: string) {
    // 检查是否有任何存档单元的任何设备路径与新路径相同
    if (save_paths.find((x) => {
        if (!x.paths) return false;
        return Object.values(x.paths).includes(p);
    })) {
        showWarning({ message: $t('addgame.duplicated_filename_error') });
        return false;
    }
    return true;
}
function check_name_valid(name: string) {
    let invalid_reg = RegExp(/[<>:"\/\\|?*]/);
    return !invalid_reg.test(name);
}
function generate_save_unit(unit_type: "Folder" | "File", path: string): SaveUnit {
    let delete_before_apply = config.value?.settings.default_delete_before_apply;
    
    // 创建一个基本的 SaveUnit，使用当前设备ID作为路径映射的键
    const saveUnit: SaveUnit = {
        unit_type,
        paths: {},
        delete_before_apply
    };
    
    // 如果有当前设备信息，则添加路径
    if (currentDevice.value) {
        const deviceId = currentDevice.value.id;
        saveUnit.paths![deviceId] = path;
    }
    
    return saveUnit;
}

// 在路径输入框中插入变量
function insertPathVariable(variable: string, inputRef: any) {
    if (!inputRef) return;
    
    const input = inputRef.$el.querySelector('input');
    if (!input) return;
    
    const start = input.selectionStart || 0;
    const end = input.selectionEnd || 0;
    const value = input.value;
    
    // 在光标位置插入变量
    const newValue = value.substring(0, start) + variable + value.substring(end);
    input.value = newValue;
    
    // 触发输入事件，确保值被更新
    input.dispatchEvent(new Event('input', { bubbles: true }));
    
    // 设置光标位置到插入的变量之后
    const newPosition = start + variable.length;
    setTimeout(() => {
        input.setSelectionRange(newPosition, newPosition);
        input.focus();
    }, 0);
}

async function add_save_directory() {
    try {
        const dir = await commands.chooseSaveDir();
        if (dir.status == "error" || !check_save_unit_unique(dir.data)) { return; }
        save_paths.push(
            generate_save_unit("Folder", dir.data)
        );
    } catch (e) {
        error(`Error choosing save directory: ${e}`)
        showError({ message: $t('error.choose_save_dir_error') });
    }
}

async function add_save_file() {
    try {
        const file = await commands.chooseSaveFile();
        if (file.status == "error" || !check_save_unit_unique(file.data)) { return; }
        save_paths.push(
            generate_save_unit("File", file.data)
        );
    } catch (e) {
        error(`Error choosing save file: ${e}`)
        showError({ message: $t('error.choose_save_file_error') });
    }
}

async function choose_executable_file() {
    try {
        const file = await commands.chooseSaveFile();
        if (file.status == "error") { return; }
        game_path.value = file.data;
    } catch (e) {
        error(`Error choosing executable file: ${e}`)
        showError({ message: $t('error.choose_executable_file_error') });
    }
}

function submit_handler(button_method: Function) {
    // 映射按钮的ID和他们要触发的方法
    button_method();
}
function search_local() {
    // TODO:导入已有配置
    showWarning({ message: $t('addgame.wip_warning') });
}
async function save() {
    // 去除头尾空字符，防止触发Windows文件命名规则问题
    game_name.value = game_name.value.trim();
    if (game_name.value == "" || save_paths.length == 0) {
        showError({ message: $t('addgame.no_name_error') });
        return;
    }
    if (!check_name_valid(game_name.value)) {
        showError({ message: $t('addgame.invalid_name_error') });
        return;
    }
    if (config.value?.games.find((x) => x.name.toLowerCase() == game_name.value.toLowerCase())) {
        showError({ message: $t('addgame.duplicated_name_error') });
        return;
    }
    let game: Game = {
        name: game_name.value,
        save_paths: save_paths,
    };

    // 如果有游戏路径和当前设备信息，则添加游戏路径
    if (game_path.value && currentDevice.value) {
        game.game_paths = {};
        game.game_paths[currentDevice.value.id] = game_path.value;
    }
    try {
        const result = await commands.addGame(game);

        if (is_editing.value) {
            is_editing.value = false;
            showSuccess({ message: $t('addgame.add_game_success') });
            router.back();
        } else {
            if (config.value?.settings.add_new_to_favorites) {
                // TODO:以下内容是否需要抽离成单独的工具库？还是说应该后端处理？
                await refreshConfig();
                config.value?.favorites?.push({
                    label: game.name,
                    is_leaf: true,
                    children: [],
                    node_id: uuidv4().toString()
                });
                await saveConfig();
            }
            showSuccess({ message: $t('addgame.add_game_success') });
        }
        reset_info(false);
        await refreshConfig();
    } catch (e) {
        error(`Error adding game: ${e}`);
        showError({ message: $t('error.add_game_failed') });
    }
}
function reset_info(show_notification: boolean = true) {
    // 重置当前配置
    game_name.value = "";
    save_paths = reactive([]);
    game_path.value = "";
    // TODO:This is a first occurrence of a i18n text duplication. How to handle this?
    if (show_notification) { showSuccess({ message: $t('settings.reset_success') }); }
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
                <div class="button-row">
                    <el-button type="primary" @click="add_save_directory">{{ $t('addgame.add_save_directory') }}</el-button>
                    <el-button type="primary" @click="add_save_file">{{ $t('addgame.add_save_file') }}</el-button>
                </div>
                <div class="path-variable-info">
                    <el-alert
                        type="info"
                        :closable="false"
                        show-icon
                    >
                        {{ $t('addgame.path_variable_hint') }}
                    </el-alert>
                </div>
            </div>
            <el-table :data="save_paths" class="save-table">
                <el-table-column fixed prop="unit_type" :label="$t('addgame.type')" width="120" />
                <el-table-column :label="$t('addgame.operations')" width="120">
                    <template #default="scope">
                        <el-button link type="primary" size="small" @click.prevent="deleteRow(scope.$index)">
                            {{ $t('addgame.remove') }} </el-button>
                    </template>
                </el-table-column>
                <el-table-column :label="$t('addgame.path')" min-width="300">
                    <template #default="scope">
                        <div class="path-input-container">
                            <el-input
                                :model-value="scope.row.paths && currentDevice ? scope.row.paths[currentDevice.id] || '' : ''"
                                size="small"
                                @update:model-value="(value) => {
                                    if (currentDevice && scope.row.paths) {
                                        scope.row.paths[currentDevice.id] = value;
                                    }
                                }"
                            >
                                <template #append>
                                    <path-variable-selector
                                        :current-path="scope.row.paths && currentDevice ? scope.row.paths[currentDevice.id] || '' : ''"
                                        @insert="(variable) => {
                                            if (currentDevice && scope.row.paths) {
                                                const currentPath = scope.row.paths[currentDevice.id] || '';
                                                scope.row.paths[currentDevice.id] = currentPath + variable;
                                            }
                                        }"
                                    />
                                </template>
                            </el-input>
                        </div>
                    </template>
                </el-table-column>
                <el-table-column :label="$t('addgame.device_info')" width="200" v-if="currentDevice">
                    <template #default>
                        <el-tag size="small">{{ currentDevice?.name }}</el-tag>
                    </template>
                </el-table-column>
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

.button-row {
    display: flex;
    gap: 10px;
    margin-bottom: 10px;
}

.path-variable-info {
    margin-top: 10px;
    margin-bottom: 10px;
}

.path-input-container {
    display: flex;
    align-items: center;
}

.submit-bar {
    justify-content: flex-end;
    height: 10%;
}
</style>