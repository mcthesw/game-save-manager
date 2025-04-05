<script lang="ts" setup>
import { computed, ref, watch } from "vue";
import { ElInput, ElMessageBox } from "element-plus";
import { useRoute, useRouter } from "vue-router";
import {commands} from "../../bindings";
import SaveLocationDrawer from "../../components/SaveLocationDrawer.vue";
import type { Game, Snapshot } from "../../bindings";
import { $t } from "../../i18n";
import { error, info } from "@tauri-apps/plugin-log";

let { showInfo, showError, showSuccess, closeNotification } = useNotification();
let { config,refreshConfig, saveConfig } = useConfig();
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
    game_path: "",
});

let describe = ref("");
let backup_button_time_limit = true; // 两次备份时间间隔1秒
let backup_button_backup_limit = true; // 上次没备份好禁止再备份或读取
let apply_button_apply_limit = true; // 上次未恢复好禁止读取或备份

// 批量操作记录列表
const selected_game_snapshots: Ref<Snapshot[]> = ref([]);

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
        refresh_backups_info()
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
    let notify_id = showInfo({ message: $t('manage.wait_for_prompt_hint') });
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

    let result = await commands.createSnapshot(game.value, describe.value);
    if (result.status === "error") {
        showError({ message: result.error });
    } else {
        showSuccess({ message: $t('manage.backup_success') });
    }
    closeNotification(notify_id);
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
    if (game.value.game_path == undefined || game.value.game_path.length < 1) {
        showError({ message: $t('manage.no_launch_path_error') });
        return;
    } else {
        let result = await commands.openUrl(game.value.game_path);
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
    let notify_id = showInfo({ message: $t('manage.wait_for_prompt_hint') });

    if (!apply_button_apply_limit) {
        showError({ message: $t('manage.last_overwrite_unfinished_error') });
        return;
    }
    if (!backup_button_backup_limit) {
        showError({ message: $t('manage.last_backup_unfinished_error') });
        return;
    }
    apply_button_apply_limit = false;
    let result = await commands.restoreSnapshot(game.value, date);
    if (result.status === "error") {
        // TODO: 增加恢复失败
        showError({ message: $t('manage.recover_failed') });
    } else {
        showSuccess({ message: $t('manage.recover_success') });
    }
    closeNotification(notify_id);
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

// 调整“应用存档位置，删除原存档”选项，由组件SaveLocationDrawer触发
async function on_save_unit_switch_delete_before_apply(index: number) {
    try {
        (config.value.games.find((x) => x.name == game.value.name) as Game).save_paths = game.value.save_paths;
        await saveConfig();
        showSuccess({ message: $t("settings.submit_success") });
        await refreshConfig();
    } catch (e) {
        error(`Failed to save config: ${e}`);
        showError({ message: $t("error.set_config_failed") });
    }
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
        <save-location-drawer v-if="game.save_paths" v-model="drawer" :locations="game.save_paths"
            @closed="drawer = false" @switched="on_save_unit_switch_delete_before_apply" />
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