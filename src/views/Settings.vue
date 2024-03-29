<script lang="ts" setup>
import { ref } from "vue";
import { useConfig } from "../stores/ConfigFile";
import { invoke } from "@tauri-apps/api/tauri";
import { show_error, show_info, show_success } from "../utils/notifications";
import { Game } from "../schemas/saveTypes";
import { useDark, useToggle } from '@vueuse/core'
import { $t } from "../i18n";
import { ElMessageBox } from "element-plus";

const isDark = useDark()
const toggleDark = useToggle(isDark)
const config = useConfig()
const loading = ref(false)

function load_config() {
    config.refresh()
}
function submit_settings() {
    loading.value = true;
    invoke("set_config", { config: config.$state }).then((x) => {
        loading.value = false;
        show_success($t("settings.submit_success"));
        load_config()
    }).catch(
        (e) => {
            console.log(e)
            show_error($t("error.set_config_failed"))
        }
    )
}
function abort_change() {
    show_success($t("settings.reset_success"));
    load_config();
}
function reset_settings() {
    invoke("reset_settings").then((x) => {
        show_success($t("settings.reset_success"));
        load_config();
    }).catch(
        (e) => {
            console.log(e)
            show_error($t("error.reset_settings_failed"))
        }
    )
}

function move_up(game: Game) {
    console.log(game)
    let index = config.games.findIndex((x) => x.name == game.name)
    if (index != 0) {
        config.games[index] = config.games.splice(index - 1, 1, config.games[index])[0];
    }
}
function move_down(game: Game) {
    let index = config.games.findIndex((x) => x.name == game.name)
    if (index != config.games.length - 1) {
        config.games[index] = config.games.splice(index + 1, 1, config.games[index])[0];
    }
}

function backup_all() {
    ElMessageBox.prompt(
        $t('settings.backup_all_hint'),
        $t('home.hint'),
        {
            confirmButtonText: $t('settings.confirm'),
            cancelButtonText: $t('settings.cancel'),
            inputPattern: /yes/,
            inputErrorMessage: $t('settings.invalid_input_error'),
        }
    )
        .then(() => {
            invoke("backup_all").then((x) => {
                show_success($t("settings.success"));
            }).catch(
                (e) => {
                    console.log(e)
                    show_error($t("error.failed"))
                }
            )
        })
        .catch(() => {
            show_info($t('setting.operation_canceled'));
        });
}

function apply_all() {
    ElMessageBox.prompt(
        $t('settings.apply_all_hint'),
        $t('home.hint'),
        {
            confirmButtonText: $t('settings.confirm'),
            cancelButtonText: $t('settings.cancel'),
            inputPattern: /yes/,
            inputErrorMessage: $t('settings.invalid_input_error'),
        }
    )
        .then(() => {
            invoke("apply_all").then((x) => {
                show_success($t("settings.success"));
            }).catch(
                (e) => {
                    console.log(e)
                    show_error($t("error.failed"))
                }
            )
        })
        .catch(() => {
            show_info($t('setting.operation_canceled'));
        });
}

</script>

<template>
    <el-container class="setting" direction="vertical">
        <el-card>
            <h1>{{ $t("settings.customizable_settings") }}</h1>
            <div class="button-bar">
                <el-button @click="submit_settings()">{{ $t("settings.submit_settings") }}</el-button>
                <el-button @click="abort_change()">{{ $t("settings.abort_change") }}</el-button>
                <el-popconfirm :title="$t('settings.confirm_reset')" :on-confirm="reset_settings">
                    <template #reference>
                        <el-button type="danger">{{ $t("settings.reset_settings") }}</el-button>
                    </template>
                </el-popconfirm>
                <el-button @click="backup_all" type="danger">
                    {{ $t("settings.backup_all") }}
                </el-button>
                <el-button @click="apply_all" type="danger">
                    {{ $t("settings.apply_all") }}
                </el-button>
            </div>
            <div class="setting-box">
                <ElSwitch v-model="config.settings.prompt_when_not_described" :loading="loading" />
                <span>{{ $t("settings.prompt_when_not_described") }}</span>
            </div>
            <div class="setting-box">
                <ElSwitch v-model="config.settings.prompt_when_auto_backup" :loading="loading" />
                <span>{{ $t("settings.prompt_when_auto_backup") }}</span>
            </div>
            <div class="setting-box">
                    <ElSwitch v-model="config.settings.exit_to_tray" :loading="loading" />
                    <span>{{ $t("settings.exit_to_tray") }}</span>
                </div>
            <div class="setting-box">
                <ElSwitch v-model="config.settings.extra_backup_when_apply" :loading="loading" />
                <span>{{ $t("settings.extra_backup_when_apply") }}</span>
            </div>
            <div class="setting-box">
                <ElSwitch :value="isDark" :loading="loading" @change="toggleDark()" />
                <span>{{ $t("settings.enable_dark_mode") }}</span>
            </div>
            <div class="setting-box">
                <ElSwitch v-model="config.settings.show_edit_button" :loading="loading" />
                <span>{{ $t("settings.enable_edit_manage") }}</span>
            </div>
            <div class="setting-box">
                <ElCollapse>
                    <ElCollapseItem :title="$t('settings.adjust_game_order')">
                        <ElTable :data="config.games" :border="true">
                            <ElTableColumn prop="name" :label="$t('settings.name')" width="180" />
                            <ElTableColumn prop="game_path" :label="$t('settings.game_path')" />
                            <ElTableColumn fixed="right" :label="$t('settings.operation')" width="120">
                                <template #default="scope">
                                    <el-button link type="primary" size="small" @click="move_up(scope.row)">{{
                                        $t("settings.move_up") }}</el-button>
                                    <el-button link type="primary" size="small" @click="move_down(scope.row)">{{
                                        $t("settings.move_down") }}</el-button>
                                </template>
                            </ElTableColumn>
                        </ElTable>
                    </ElCollapseItem>
                </ElCollapse>
            </div>
        </el-card>
    </el-container>
</template>

<style scoped>
.el-button {
    margin-left: 0px !important;
    margin-right: 10px;
    margin-top: 5px;
}
.el-card{
    overflow-y: scroll;
}
.el-switch {
    margin-right: 20px;
}

.setting-box {
    margin-top: 10px;
}
</style>
