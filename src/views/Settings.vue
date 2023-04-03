<script lang="ts" setup>
import { Config,default_config } from "../schemas/saveTypes";
import { ElNotification } from "element-plus";
import { reactive, ref } from "vue";
import { useConfig } from "../stores/ConfigFile";


const config: Config = reactive(JSON.parse(JSON.stringify(default_config)))
const loading = ref(false)

function load_config() {
    // TODO:读取已有Config
    // ipcRenderer.send("get_config");
    // setTimeout(set_view_config, 100);
}
function submit_config() {
    loading.value = true;
    // TODO:提交config
    // ipcRenderer.send("set_config", config);
}
function abort_change() {
    ElNotification({
        type: "success",
        message: "重置成功",
    });
    load_config();
}
function reset_settings() {
    config.settings = JSON.parse(
        JSON.stringify(default_config)
    ).settings;
    ElNotification({
        type: "warning",
        message: "注意，保存后该重置才会生效",
    });
}
//TODO: 需要仔细确认


</script>

<template>
    <el-container class="setting" direction="vertical">
        <el-card>
            <h1>个性化设置</h1>
            <el-button @click="submit_config()">保存修改</el-button>
            <el-button @click="abort_change()">放弃修改</el-button>
            <el-button @click="reset_settings()" type="danger">恢复默认配置</el-button>
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
