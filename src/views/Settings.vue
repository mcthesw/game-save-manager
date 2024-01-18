<script lang="ts" setup>
import { ref } from "vue";
import { useConfig } from "../stores/ConfigFile";
import { invoke } from "@tauri-apps/api/tauri";
import { show_success } from "../utils/notifications";
import { Game } from "../schemas/saveTypes";
import { useDark, useToggle } from '@vueuse/core'

const isDark = useDark()
let isDarkDisplay = ref(isDark)
const toggleDark = useToggle(isDark)
const config = useConfig()
const loading = ref(false)

function load_config() {
    config.refresh()
}
function submit_config() {
    loading.value = true;
    invoke("set_config", { config: config }).then((x) => {
        loading.value = false;
        console.log(x);
        show_success("设置成功");
        load_config()
    }).catch(
        (e) => { console.log(e) }
    )
}
function abort_change() {
    show_success("重置成功");
    load_config();
}
function reset_settings() {
    invoke("reset_settings").then((x) => {
        show_success("重置成功");
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
            <h1>个性化设置</h1>
            <el-button @click="submit_config()">保存修改</el-button>
            <el-button @click="abort_change()">放弃修改</el-button>
            <el-popconfirm title="确认重置?" :on-confirm="reset_settings">
                <template #reference>
                    <el-button type="danger">恢复默认配置</el-button>
                </template>
            </el-popconfirm>
            <br />
            <div class="setting-box">
                <ElSwitch v-model="config.settings.prompt_when_not_described" :loading="loading" />
                <span>当未描述存档时，弹出提示</span>
            </div>
            <div class="setting-box">
                <ElSwitch v-model="config.settings.extra_backup_when_apply" :loading="loading" />
                <span>在应用存档时进行额外备份（在 ./save_data/游戏名/extra_backup 文件夹内）</span>
            </div>
            <div class="setting-box">
                <ElSwitch :value="isDarkDisplay" :loading="loading" @change="toggleDark()" />
                <span>启用夜间模式</span>
            </div>
            <div class="setting-box">
                <ElCollapse>
                    <ElCollapseItem title="调整游戏展示顺序（需要保存设置）">
                        <ElTable :data="config.games" :border="true">
                            <ElTableColumn prop="name" label="游戏名" width="180" />
                            <ElTableColumn prop="game_path" label="启动路径" />
                            <ElTableColumn fixed="right" label="操作" width="120">
                                <template #default="scope">
                                    <el-button link type="primary" size="small" @click="move_up(scope.row)">上移</el-button>
                                    <el-button link type="primary" size="small" @click="move_down(scope.row)">下移</el-button>
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
