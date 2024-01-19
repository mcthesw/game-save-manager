<script lang="ts" setup>
import { ref } from "vue";
import { useConfig } from "../stores/ConfigFile";
import { invoke } from "@tauri-apps/api/tauri";
import { show_success } from "../utils/notifications";
import { Game } from "../schemas/saveTypes";
import { useDark, useToggle } from '@vueuse/core'
import { $t } from "../i18n";

const isDark = useDark()
const toggleDark = useToggle(isDark)
const config = useConfig()
const loading = ref(false)

function load_config() {
    config.refresh()
}
function submit_settings() {
    loading.value = true;
    invoke("set_config", { config: config }).then((x) => {
        loading.value = false;
        console.log(x);
        show_success($t("reset_success"));
        load_config()
    }).catch(
        (e) => { console.log(e) }
    )
}
function abort_change() {
    show_success($t("reset_success"));
    load_config();
}
function reset_settings() {
    invoke("reset_settings").then((x) => {
        show_success($t("reset_success"));
        load_config();
    }).catch(
        (e) => { console.log(e) }
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

</script>

<template>
    <el-container class="setting" direction="vertical">
        <el-card>
            <h1>{{ $t("customizable_settings") }}</h1>
            <el-button @click="submit_settings()">{{ $t("submit_settings") }}</el-button>
            <el-button @click="abort_change()">{{ $t("abort_change") }}</el-button>
            <el-popconfirm :title="$t('confirm_reset')" :on-confirm="reset_settings">
                <template #reference>
                    <el-button type="danger">{{ $t("reset_settings") }}</el-button>
                </template>
            </el-popconfirm>
            <br />
            <div class="setting-box">
                <ElSwitch v-model="config.settings.prompt_when_not_described" :loading="loading" />
                <span>{{ $t("prompt_when_not_described") }}</span>
            </div>
            <div class="setting-box">
                <ElSwitch v-model="config.settings.extra_backup_when_apply" :loading="loading" />
                <span>{{ $t("extra_backup_when_apply") }}</span>
            </div>
            <div class="setting-box">
                <ElSwitch :value="isDark" :loading="loading" @change="toggleDark()" />
                <span>{{ $t("enable_dark_mode") }}</span>
            </div>
            <div class="setting-box">
                <ElCollapse>
                    <ElCollapseItem :title="$t('adjust_game_order')">
                        <ElTable :data="config.games" :border="true">
                            <ElTableColumn prop="name" :label="$t('name')" width="180" />
                            <ElTableColumn prop="game_path" :label="$t('game_path')" />
                            <ElTableColumn fixed="right" :label="$t('operation')" width="120">
                                <template #default="scope">
                                    <el-button link type="primary" size="small" @click="move_up(scope.row)">{{ $t("move_up") }}</el-button>
                                    <el-button link type="primary" size="small" @click="move_down(scope.row)">{{ $t("move_down") }}</el-button>
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
.el-switch {
    margin-right: 20px;
}

.setting-box {
    margin-top: 10px;
}
</style>
