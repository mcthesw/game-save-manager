<script lang="ts" setup>
import { computed, ref, watch, onBeforeUnmount, onMounted } from "vue";
import { ElInput, ElMessageBox } from "element-plus";
import { useRoute, useRouter } from "vue-router";
import { commands, events } from "../../bindings";
import SaveLocationDrawer from "../../components/SaveLocationDrawer.vue";
import type { Game, Snapshot, Device, SaveUnit } from "../../bindings";
import { $t } from "../../i18n";
import { error, info } from "@tauri-apps/plugin-log";

let { showInfo, showError, showSuccess, closeNotification } = useNotification();
let { config, refreshConfig, saveConfig } = useConfig();
const { withLoading } = useGlobalLoading();
let router = useRouter();
let route = useRoute();
const top_buttons = [
    { text: $t('manage.create_new_save'), method: create_new_save },
    { text: $t('manage.load_latest_save'), method: load_latest_save },
    { text: $t('manage.launch_game'), method: launch_game },
    { text: $t('manage.open_backup_folder'), method: open_backup_folder },
    { text: $t('manage.show_drawer'), method: () => { drawer.value = !drawer.value; } },
    { text: $t('manage.set_quick_backup'), method: set_quick_backup }
]

const search = ref(""); // 搜索时使用的字符串
const drawer = ref(false); // 是否显示存档位置侧栏

let table_data = ref([
    {
        date: "",
        describe: $t('manage.error_info'),
        path: "",
    },
]);

let game: Ref<Game> = ref({
    name: "",
    save_paths: [],
    game_paths: {},
});

// 当前设备信息
const currentDevice = ref<Device | null>(null);

// 获取当前设备信息
async function fetchCurrentDevice() {
    try {
        const result = await commands.getCurrentDeviceInfo();
        if (result.status === "ok") {
            currentDevice.value = result.data;
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

let describe = ref("");
let backup_button_time_limit = true; // 两次备份时间间隔1秒
let backup_button_backup_limit = true; // 上次没备份好禁止再备份或读取
let apply_button_apply_limit = true; // 上次未恢复好禁止读取或备份

// 批量操作记录列表
const selected_game_snapshots: Ref<Snapshot[]> = ref([]);

let stopQuickActionListener: (() => void) | null = null;

onMounted(async () => {
    try {
        stopQuickActionListener = await events.quickActionCompleted.listen(
            async (event) => {
                const payload = event.payload;
                if (
                    payload.status === 'Success' &&
                    payload.operation === 'Backup' &&
                    payload.game_name &&
                    payload.game_name === game.value.name
                ) {
                    await refresh_backups_info();
                }
            },
        );
    } catch (e) {
        error(`Failed to listen quick action events: ${e}`);
    }
});

onBeforeUnmount(() => {
    if (stopQuickActionListener) {
        stopQuickActionListener();
        stopQuickActionListener = null;
    }
});

// 格式化文件大小显示
function formatFileSize(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}
function on_selection_change(val: Snapshot[]) {
    selected_game_snapshots.value = val;
}
async function batch_delete() {
    try {
        const result = await ElMessageBox.prompt(
            $t('manage.batch_delete_prompt'),
            $t('home.hint'),
            {
                confirmButtonText: $t('manage.confirm'),
                cancelButtonText: $t('manage.cancel'),
                inputPattern: /yes/,
                inputErrorMessage: $t('manage.invalid_input_error'),
            }
        );

        if (result.value === 'yes') {
            for (const item of selected_game_snapshots.value) {
                await del_save(item.date);
            }
        } else {
            showInfo({ message: $t('manage.invalid_input_error') });
        }
    } catch (error) {
        showError({ message: $t('manage.operation_canceled') });
    }
}

// Init game info
watch(
    () => route.params.name,
    (newValue) => {
        if (!newValue) { return; }
        let name = newValue;
        game.value = config.value.games.find((x) => x.name == name) as Game;
        refresh_backups_info();
        // 检查当前设备的存档路径是否为空
        checkCurrentDeviceSavePaths();
    },
    { immediate: true }
)

async function refresh_backups_info() {
    let result = await commands.getGameSnapshotsInfo(game.value);
    if (result.status === "error") {
        showError({ message: result.error });
    } else {
        table_data.value = result.data.backups;
    }
}

async function send_save_to_background() {
    showInfo({ message: $t('manage.wait_for_prompt_hint') });
    if (!backup_button_time_limit) {
        showError({ message: $t('manage.save_too_fast_error') });
        return;
    }
    if (!backup_button_backup_limit) {
        showError({ message: $t('manage.last_backup_unfinished_error') });
        return;
    }
    if (!apply_button_apply_limit) {
        showError({ message: $t('manage.last_overwrite_unfinished_error') });
        return;
    }
    backup_button_time_limit = false;
    backup_button_backup_limit = false;

    await withLoading(async () => {
        let result = await commands.createSnapshot(game.value, describe.value);
        if (result.status === "error") {
            showError({ message: result.error });
        } else {
            showSuccess({ message: $t('manage.backup_success') });
        }
    }, $t('manage.creating_backup'));
    backup_button_backup_limit = true;
    refresh_backups_info();

    describe.value = "";
    setTimeout(() => {
        backup_button_time_limit = true;
    }, 1000);
}

async function create_new_save() {
    if (
        config.value.settings.prompt_when_not_described && !describe.value
    ) {
        try {
            await ElMessageBox.confirm($t('manage.no_description_warning'), $t('manage.warning'), {
                confirmButtonText: $t('manage.confirm_save'),
                cancelButtonText: $t('manage.cancel'),
                type: "warning",
            });
            send_save_to_background();
        } catch (e) {
            info(`User cancelled the save operation.`);
        }
    } else {
        send_save_to_background();
    }
}

async function launch_game() {
    // 获取当前设备的游戏路径
    let gamePath = "";
    if (currentDevice.value && game.value.game_paths) {
        gamePath = game.value.game_paths[currentDevice.value.id] || "";
    }

    if (!gamePath) {
        showError({ message: $t('manage.no_launch_path_error') });
        return;
    } else {
        let result = await commands.openFileOrFolder(gamePath);
        if (result.status === "error") {
            showError({ message: result.error });
        }
    }
}

async function del_save(date: string) {
    try {
        const result = await commands.deleteSnapshot(game.value, date);
        refresh_backups_info();
        showSuccess({ message: $t('manage.delete_success') });
    } catch (e) {
        error(`Failed to delete snapshot: ${e}`);
        showError({ message: $t('error.delete_snapshot_failed') });
    }
}

async function apply_save(date: string) {
    showInfo({ message: $t('manage.wait_for_prompt_hint') });

    if (!apply_button_apply_limit) {
        showError({ message: $t('manage.last_overwrite_unfinished_error') });
        return;
    }
    if (!backup_button_backup_limit) {
        showError({ message: $t('manage.last_backup_unfinished_error') });
        return;
    }
    apply_button_apply_limit = false;
    await withLoading(async () => {
        let result = await commands.restoreSnapshot(game.value, date);
        if (result.status === "error") {
            showError({ message: $t('manage.recover_failed') });
        } else {
            showSuccess({ message: $t('manage.recover_success') });
        }
    }, $t('manage.restoring_backup'));
    apply_button_apply_limit = true;
    refresh_backups_info();
}

async function change_describe(date: string) {
    try {
        const { value } = await ElMessageBox.prompt($t('manage.input_description_prompt'), $t('manage.change_description'), {
            confirmButtonText: $t('manage.confirm'),
            cancelButtonText: $t('manage.cancel'),
            inputValue: table_data.value.find((x) => x.date == date)?.describe,
        });
        let result = await commands.setSnapshotDescription(game.value, date, value);
        if (result.status === "error") {
            // TODO: 增加文本
            showError({ message: $t('manage.change_description_failed') });
        }
        refresh_backups_info();
        showSuccess({ message: $t('manage.change_description_success') });
    } catch {
        showInfo({ message: $t('manage.operation_canceled') });
    }
}

function load_latest_save() {
    // 数组是正序的，最后一个是最新的，而展示用的filter_table是倒序的
    if (table_data.value[table_data.value.length - 1].date) {
        apply_save(table_data.value[table_data.value.length - 1].date);
    } else {
        showError({ message: $t('manage.no_backup_error') });
    }
}

async function del_cur() {
    try {
        const { value } = await ElMessageBox.prompt(
            $t('manage.delete_prompt'),
            $t('home.hint'),
            {
                confirmButtonText: $t('manage.confirm'),
                cancelButtonText: $t('manage.cancel'),
                inputPattern: /yes/,
                inputErrorMessage: $t('manage.invalid_input_error'),
            }
        );

        if (value === 'yes') {
            let result = await commands.deleteGame(game.value);
            if (result.status === "error") {
                showError({ message: $t('error.delete_game_failed') });
            }
            await refreshConfig();
            router.back();
        } else {
            showInfo({ message: $t('manage.invalid_input_error') });
        }
    } catch {
        showInfo({ message: $t('manage.operation_canceled') });
    }
}

async function open_backup_folder() {

    let result = await commands.openBackupFolder(game.value);
    if (result.status === "error") {
        showError({ message: $t('error.open_backup_folder_failed') });
    }
}

// 点击按钮后，跳转到添加游戏页面
async function edit_cur() {
    try {
        const { value } = await ElMessageBox.prompt(
            $t('manage.change_prompt'),
            $t('misc.info'),
            {
                confirmButtonText: $t('manage.confirm'),
                cancelButtonText: $t('manage.cancel'),
                inputPattern: /yes/,
                inputErrorMessage: $t('manage.invalid_input_error'),
            }
        );

        if (value === 'yes') {
            await refreshConfig();
            router.push({
                name: "edit-game",
                params: {
                    name: game.value.name,
                },
            });
        } else {
            showInfo({ message: $t('manage.invalid_input_error') });
        }
    } catch {
        showInfo({ message: $t('manage.operation_canceled') });
    }
}

// 设置快速备份，由快捷键和tray触发备份和恢复
async function set_quick_backup() {
    let result = await commands.setQuickBackupGame(game.value);
    if (result.status === "error") {
        showError({ message: $t('manage.set_quick_backup_failed') });
        return;
    }
    showSuccess({ message: $t('manage.set_quick_backup_success') });
}

// 处理抽屉组件保存游戏路径的事件
async function on_drawer_save_changes(updatedGame: Game) {
    // 更新游戏信息（包括存档路径和启动路径）
    game.value = updatedGame;

    // 保存到配置
    const index = config.value.games.findIndex(g => g.name === game.value.name);
    if (index !== -1) {
        config.value.games[index] = game.value;
        saveConfig().then(() => {
            showSuccess({ message: $t('manage.save_paths_updated') });
        }).catch((e) => {
            error(`Error saving config: ${e}`);
            showError({ message: $t('error.save_config_failed') });
        });
    } else {
        showError({ message: $t('error.game_not_found') });
    }

    // 关闭侧栏
    drawer.value = false;
}

const filter_table = computed(
    () => {
        return table_data.value.filter(
            (data) =>
                !search.value ||
                data.describe.includes(search.value) ||
                data.date.includes(search.value)
        ).reverse();
    }
)

// 检查当前设备的存档路径是否为空
async function checkCurrentDeviceSavePaths() {
    await fetchCurrentDevice();
    if (!currentDevice.value || !game.value || !game.value.save_paths) return;

    // 检查当前设备的存档路径是否全部为空
    const deviceId = currentDevice.value.id;
    const allPathsEmpty = game.value.save_paths.every(unit =>
        !unit.paths || !unit.paths[deviceId] || unit.paths[deviceId].trim() === ''
    );

    if (!allPathsEmpty) return; // 如果有路径不为空，直接返回

    // 收集所有有效的设备ID（有存档路径的设备）
    const devicesWithPaths = new Set<string>();
    game.value.save_paths.forEach(unit => {
        if (unit.paths) {
            Object.entries(unit.paths).forEach(([id, path]) => {
                if (id !== deviceId && path && path.trim() !== '') {
                    devicesWithPaths.add(id);
                }
            });
        }
    });

    if (devicesWithPaths.size === 0) return; // 如果没有其他设备有路径，直接返回

    try {
        // 询问用户是否要复制其他设备的存档路径
        const confirmResult = await ElMessageBox.confirm(
            $t('manage.empty_paths_prompt'),
            $t('manage.empty_paths_title'),
            {
                confirmButtonText: $t('manage.copy_from_device'),
                cancelButtonText: $t('manage.keep_empty'),
                type: 'info',
                closeOnClickModal: false,
                closeOnPressEscape: false,
            }
        );

        if (confirmResult !== 'confirm') return;

        // 准备设备选择列表
        const deviceOptions = Array.from(devicesWithPaths).map(id => ({
            value: id,
            label: id.substring(0, 8) + '...'
        }));

        // 如果只有一个设备，直接使用它
        if (deviceOptions.length === 1) {
            await copyPathsFromDevice(deviceOptions[0].value);
            return;
        }

        // 让用户从多个设备中选择
        try {
            // 显示设备列表供用户选择
            const items = deviceOptions.map((d, index) =>
                `${index + 1}. ${d.label} (${d.value})`
            ).join('\n');

            const { value } = await ElMessageBox.prompt(
                `${$t('manage.select_device_prompt')}\n\n${items}\n\n${$t('manage.enter_device_id')}:`,
                $t('manage.select_device_title'),
                {
                    confirmButtonText: $t('manage.confirm'),
                    cancelButtonText: $t('manage.cancel'),
                }
            );

            // 查找匹配的设备ID
            const selectedDevice = deviceOptions.find(d =>
                d.value === value || d.value.startsWith(value) || d.label.includes(value)
            );

            if (selectedDevice) {
                await copyPathsFromDevice(selectedDevice.value);
            }
        } catch (e) {
            // 用户取消选择，不执行任何操作
        }
    } catch (e) {
        // 用户取消初始确认，不执行任何操作
    }
}

// 从指定设备复制存档路径到当前设备
async function copyPathsFromDevice(sourceDeviceId: string) {
    if (!currentDevice.value || !game.value) return;

    const targetDeviceId = currentDevice.value.id;
    let updated = false;

    // 复制存档路径
    if (game.value.save_paths) {
        game.value.save_paths.forEach(unit => {
            if (unit.paths?.[sourceDeviceId]?.trim()) {
                if (!unit.paths[targetDeviceId] || !unit.paths[targetDeviceId].trim()) {
                    if (!unit.paths) unit.paths = {};
                    unit.paths[targetDeviceId] = unit.paths[sourceDeviceId];
                    updated = true;
                }
            }
        });
    }

    // 复制游戏启动路径
    if (game.value.game_paths?.[sourceDeviceId]?.trim() &&
        (!game.value.game_paths[targetDeviceId] || !game.value.game_paths[targetDeviceId].trim())) {
        if (!game.value.game_paths) game.value.game_paths = {};
        game.value.game_paths[targetDeviceId] = game.value.game_paths[sourceDeviceId];
        updated = true;
    }

    // 如果有更新，保存配置
    if (updated) {
        const index = config.value.games.findIndex(g => g.name === game.value.name);
        if (index !== -1) {
            config.value.games[index] = game.value;
            try {
                await saveConfig();
                showSuccess({ message: $t('manage.paths_copied_success') });
                // 打开侧栏让用户查看和编辑复制的路径
                drawer.value = true;
            } catch (e) {
                error(`Error saving config: ${e}`);
                showError({ message: $t('error.save_config_failed') });
            }
        }
    }
}
</script>

<template>
    <div class="manage-container">
        <!-- 下面是顶栏部分 -->
        <el-card class="manage-top-bar">
            <div class="button-bar">
                <template v-for="button in top_buttons" :key="button.text">
                    <el-button type="primary" round @click="button.method">
                        {{ button.text }}
                    </el-button>
                </template>

                <!-- TODO: 移除该功能 -->
                <!-- <el-button v-if="showEditButton" type="danger" round @click="edit_cur()">
                    {{ $t('manage.change_info') }}
                </el-button> -->
                <el-button type="danger" round @click="del_cur()">
                    {{ $t('manage.delete_save_manage') }}
                </el-button>
                <el-button type="danger" round v-if="selected_game_snapshots.length > 0" @click="batch_delete()">
                    {{ $t("manage.batch_delete") }}
                </el-button>
            </div>
            <!-- 下面是当前存档描述信息 -->
            <el-form @submit.prevent="create_new_save">
                <el-input v-model="describe" :placeholder="$t('manage.input_description_prompt')">
                    <template #prepend>{{ game.name + $t('manage.new_save_of') }} </template>
                </el-input>
            </el-form>
        </el-card>
        <!-- 下面是主体部分 -->
        <el-card class="saves-container">
            <!-- 存档应当用点击展开+内部表格的方式来展示 -->
            <!-- 这里应该有添加新存档按钮，按下后选择标题和描述进行存档 -->
            <el-table :data="filter_table" style="width: 100%" @selection-change="on_selection_change">
                <el-table-column type="selection" width="55" />
                <el-table-column :label="$t('manage.save_date')" prop="date" width="200px" sortable />
                <el-table-column :label="$t('manage.description')" prop="describe" />
                <el-table-column :label="$t('manage.size')" width="120px">
                    <template #default="scope">
                        <span v-if="scope.row.size && scope.row.size > 0">
                            {{ formatFileSize(scope.row.size) }}
                        </span>
                        <span v-else class="text-muted">
                            {{ $t('manage.size_not_available') }}
                        </span>
                    </template>
                </el-table-column>
                <el-table-column align="right">
                    <template #header>
                        <!-- 搜索 -->
                        <el-input v-model="search" size="small"
                            :placeholder="$t('manage.input_description_search_prompt')" clearable />
                    </template>
                    <template #default="scope">
                        <!-- scope.$index和scope.row可以被使用 -->
                        <el-popconfirm :title="$t('manage.confirm_overwrite_prompt')"
                            @confirm="apply_save(scope.row.date)">
                            <template #reference>
                                <el-button size="small"> {{ $t('manage.apply') }} </el-button>
                            </template>
                        </el-popconfirm>
                        <el-button size="small" @click="change_describe(scope.row.date)">
                            {{ $t('manage.change_describe') }}
                        </el-button>
                        <el-popconfirm :title="$t('manage.confirm_delete_prompt')" @confirm="del_save(scope.row.date)">
                            <template #reference>
                                <el-button size="small" type="danger">
                                    {{ $t('manage.delete') }} </el-button>
                            </template>
                        </el-popconfirm>
                    </template>
                </el-table-column>
            </el-table>
        </el-card>
        <!-- 下面是存档所在位置侧栏部分 -->
        <save-location-drawer v-if="game" v-model="drawer" :game="game" @closed="drawer = false"
            @save-changes="on_drawer_save_changes" />
    </div>
</template>

<style scoped>
.el-button {
    margin-left: 10px !important;
    margin-top: 5px;
}

.manage-top-bar {
    width: 98%;
    padding-right: 10px;
    padding-left: 10px;
    margin: auto auto 5px;

    display: flex;
    border-radius: 10px;
    align-items: center;
    color: aliceblue;
}

.manage-top-bar .el-input {
    margin-top: 15px;
}

.saves-container {
    margin: auto;
}
</style>