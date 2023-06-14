<script lang="ts" setup>
import { ref } from "vue";
import { useConfig } from "../stores/ConfigFile";
import { invoke } from "@tauri-apps/api/tauri";
import { show_success } from "../utils/notifications";


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
