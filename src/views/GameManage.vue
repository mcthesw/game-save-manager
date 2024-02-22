<script lang="ts" setup>
import { Ref, computed, ref, watch } from "vue";
import { ElInput, ElMessageBox } from "element-plus";
import { invoke } from "@tauri-apps/api/tauri";
import { useConfig } from "../stores/ConfigFile";
import { BackupsInfo, Game } from "../schemas/saveTypes";
import { useRoute, useRouter } from "vue-router";
import { show_error, show_info, show_success, show_warning } from "../utils/notifications";
import SaveLocationDrawer from "../components/SaveLocationDrawer.vue";
import { $t } from "../i18n";

let config = useConfig();
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
let showEditButton = config.settings.show_edit_button;

// Init game info
watch(
    () => route.params,
    (newValue) => {
        if (!newValue.name) { return; }
        let name = newValue.name;
        console.log("Current game:", name)
        game.value = config.games.find((x) => x.name == name) as Game;
        refresh_backups_info()
    },
    { immediate: true }
)

function refresh_backups_info() {
    invoke("get_backup_list_info", { game: game.value })
        .then((v) => {
            let infos = v as BackupsInfo;
            table_data.value = infos.backups;
            console.log("Backup infos:", v)
        }).catch(
            (e) => {
                console.log(e)
                show_error($t('error.get_backup_list_failed'));
            }
        )
}

function send_save_to_background() {
    show_info($t('manage.wait_for_prompt_hint'));
    if (!backup_button_time_limit) {
        show_error($t('manage.save_too_fast_error'));
        return;
    }
    if (!backup_button_backup_limit) {
        show_error($t('manage.last_backup_unfinished_error'));
        return;
    }
    if (!apply_button_apply_limit) {
        show_error($t('manage.last_overwrite_unfinished_error'));
        return;
    }
    backup_button_time_limit = false;
    backup_button_backup_limit = false;
    invoke("backup_save", { game: game.value, describe: describe.value })
        .then((_) => {
            show_success($t('manage.backup_success'));
        }).catch(
            (e) => {
                console.log(e)
                show_error($t('error.backup_failed'))
            }
        ).finally(() => {
            backup_button_backup_limit = true
            refresh_backups_info();
        })

    describe.value = "";
    setTimeout(() => {
        backup_button_time_limit = true;
    }, 1000);
}

function create_new_save() {
    if (
        config.settings.prompt_when_not_described && !describe.value
    ) {
        ElMessageBox.confirm($t('manage.no_description_warning'), $t('manage.warning'), {
            confirmButtonText: $t('manage.confirm_save'),
            cancelButtonText: $t('manage.cancel'),
            type: "warning",
        })
            .then(() => {
                send_save_to_background();
            })
            .catch(() => { });
    } else {
        send_save_to_background();
    }
}

function launch_game() {
    if (game.value.game_path == undefined || game.value.game_path.length < 1) {
        show_error($t('manage.no_launch_path_error'));
        return;
    } else {
        invoke("open_url", { url: game.value.game_path })
            .then((x) => {
                console.log(x)
            }).catch(
                (e) => {
                    console.log(e)
                    show_error($t("error.open_url_failed"))
                }
            )
    }
}

function del_save(date: string) {
    console.log(date)
    invoke("delete_backup", { game: game.value, date: date }).then((x) => {
        console.log(x)
        refresh_backups_info();
        show_success($t('manage.delete_success'));
    }).catch(
        (e) => {
            console.log(e)
            show_error($t('error.delete_backup_failed'))
        }
    )
}

function apply_save(date: string) {
    show_warning($t('manage.wait_for_prompt_hint'));

    if (!apply_button_apply_limit) {
        show_error($t('manage.last_overwrite_unfinished_error'));
        return;
    }
    if (!backup_button_backup_limit) {
        show_error($t('manage.last_backup_unfinished_error'))
        return;
    }
    apply_button_apply_limit = false;
    invoke("apply_backup", { game: game.value, date: date })
        .then((x) => {
            show_success($t('manage.recover_success'));
            console.log(x)
        }).catch((e) => {
            console.log(e)
            show_error($t('error.apply_backup_failed'))
        }).finally(() => {
            apply_button_apply_limit = true;
            refresh_backups_info();
        })
}

function change_describe(date: string) {
    ElMessageBox.prompt($t('manage.input_description_prompt'), $t('manage.change_description'), {
        confirmButtonText: $t('manage.confirm'),
        cancelButtonText: $t('manage.cancel'),
        inputValue: table_data.value.find((x) => x.date == date)?.describe,
    })
        .then(({ value }) => {
            invoke("set_backup_describe", { game: game.value, date: date, describe: value })
                .then((x) => {
                    console.log(x)
                    refresh_backups_info();
                    show_success($t('manage.change_description_success'));
                }).catch(
                    (e) => {
                        console.log(e)
                        show_error($t('error.change_description_failed'))
                    }
                )
        })
        .catch(() => {
            show_info($t('manage.operation_canceled'));
        });
}

function load_latest_save() {
    // 数组是正序的，最后一个是最新的，而展示用的filter_table是倒序的
    if (table_data.value[table_data.value.length-1].date) {
        apply_save(table_data.value[table_data.value.length - 1].date);
    } else {
        show_error($t('manage.no_backup_error'));
    }
}

function del_cur() {
    ElMessageBox.prompt(
        $t('manage.delete_prompt'),
        $t('home.hint'),
        {
            confirmButtonText: $t('manage.confirm'),
            cancelButtonText: $t('manage.cancel'),
            inputPattern: /yes/,
            inputErrorMessage: $t('manage.invalid_input_error'),
        }
    )
        .then(() => {
            invoke("delete_game", { game: game.value }).catch((e) => {
                console.log(e)
                show_error($t('error.delete_game_failed'))
            });
            setTimeout(() => {
                config.refresh()
                router.back()
            }, 100)
        })
        .catch(() => {
            show_info($t('manage.operation_canceled'));
        });
}

function open_backup_folder() {
    invoke("open_backup_folder", { game: game.value })
        .catch(
            (e) => {
                console.log(e)
                show_error($t('error.open_backup_folder_failed'))
            }
        )
}

// 点击按钮后，跳转到添加游戏页面
function edit_cur() {
    ElMessageBox.prompt(
        $t('manage.change_prompt'),
        $t('misc.info'),
        {
            confirmButtonText: $t('manage.confirm'),
            cancelButtonText: $t('manage.cancel'),
            inputPattern: /yes/,
            inputErrorMessage: $t('manage.invalid_input_error'),
        }
    )
        .then(() => {
            config.refresh()
            router.push({
                name: "edit-game",
                params: {
                    name: game.value.name,
                },
            });
        })
        .catch(() => {
            show_info($t('manage.operation_canceled'));
        });
}

// 设置快速备份，由快捷键和tray触发备份和恢复
function set_quick_backup() {
    invoke("set_quick_backup_game", { game: game.value })
        .then((x) => {
            console.log(x)
            show_success($t('manage.set_quick_backup_success'));
        }).catch(
            (e) => {
                console.log(e)
                show_error($t('manage.set_quick_backup_failed'))
            }
        )
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

                <el-button v-if="showEditButton" type="danger" round @click="edit_cur()">
                    {{ $t('manage.change_info') }}
                </el-button>
                <el-button type="danger" round @click="del_cur()">
                    {{ $t('manage.delete_save_manage') }}
                </el-button>
            </div>
            <!-- 下面是当前存档描述信息 -->

            <el-input v-model="describe" :placeholder="$t('manage.input_description_prompt')">
                <template #prepend>{{ game.name + $t('manage.new_save_of') }} </template>
            </el-input>
        </el-card>
        <!-- 下面是主体部分 -->
        <el-card class="saves-container">
            <!-- 存档应当用点击展开+内部表格的方式来展示 -->
            <!-- 这里应该有添加新存档按钮，按下后选择标题和描述进行存档 -->
            <el-table :data="filter_table" style="width: 100%">
                <el-table-column :label="$t('manage.save_date')" prop="date" width="200px" sortable />
                <el-table-column :label="$t('manage.description')" prop="describe" />
                <el-table-column align="right">
                    <template #header>
                        <!-- 搜索 -->
                        <el-input v-model="search" size="small" :placeholder="$t('manage.input_description_search_prompt')"
                            clearable />
                    </template>
                    <template #default="scope">
                        <!-- scope.$index和scope.row可以被使用 -->
                        <el-popconfirm :title="$t('manage.confirm_overwrite_prompt')" @confirm="apply_save(scope.row.date)">
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
            @closed="drawer = false" />
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