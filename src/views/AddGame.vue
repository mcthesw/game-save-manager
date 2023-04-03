<script lang="ts" setup>
// !为了不让 submit_handler 报错，这里不使用 lang="ts"
import { DocumentAdd } from "@element-plus/icons-vue";
import {
    Check,
    RefreshRight,
    Download,
    MagicStick,
} from "@element-plus/icons-vue";
import { ElNotification } from "element-plus";
import { reactive, ref } from "vue";
import { useRouter } from "vue-router";
const router = useRouter();

const buttons = [
    {
        text: "修改已保存的配置",
        type: "",
        icon: MagicStick,
        method: change,
    },
    {
        text: "自动识别本地游戏",
        type: "primary",
        icon: Download,
        method: search_local,
    },
    {
        text: "保存当前编辑的配置",
        type: "success",
        icon: Check,
        method: save,
    },
    {
        text: "重置当前配置",
        type: "danger",
        icon: RefreshRight,
        method: reset,
    },
] as const;


const game_name = ref("") // 写入游戏名
let save_path = reactive([]) // 选择游戏存档目录
const game_path = ref("") // 选择游戏启动程序
const game_icon_src = ref("https://shadow.elemecdn.com/app/element/hamburger.9cf7b091-55e9-11e9-a976-7f4d0b07eef6.png")

function choose_save_directory() {
    // TODO:选择游戏存档目录
}
function choose_executable_file() {
    // TODO:选择游戏启动程序
}
function choose_game_icon() {
    // TODO:选择游戏图标地址
    // TODO:需要优化图片选择，自由切割和压缩到指定大小
}
function submit_handler(button_method: Function) {
    // TODO:映射按钮的ID和他们要触发的方法
    button_method();
}
function search_local() {
    // TODO:导入已有配置
    ElNotification({
        type: "warning",
        message: "--WIP-- 这个功能尚未完成",
    });
}
function change() {
    router.push("/change-game-info");
}
function save() {
    if (game_name.value == "" || save_path.length == 0) {
        ElNotification({
            type: "error",
            message: "请至少输入游戏名和一个存档路径"
        })
        return
    }
    // TODO保存当前配置
    console.log("保存当前编辑的配置");
    // ipcRenderer.send("add_game", {
    // 	game_name: game_name.value,
    // 	save_path: save_path.value,
    // 	icon: game_icon_src.value,
    // 	game_path: game_path.value,
    // });
}
function reset() {
    // 重置当前配置
    game_name.value = "";
    save_path = reactive([]);
    game_path.value = "";
    game_icon_src.value =
        "https://shadow.elemecdn.com/app/element/hamburger.9cf7b091-55e9-11e9-a976-7f4d0b07eef6.png";
    ElNotification({
        title: "提示",
        message: "重置成功",
        type: "success",
        duration: 1000,
    });
}

function deleteRow(index: number) {
    save_path.splice(index, 1);
}
function addPath() {
    // save_path.push({
    //     path: "",
    //     type: "",
    // });
}
</script>

<template>
    <div class="select-container">
        <el-card class="game-info">
            <div class="top-part">
                <img class="game-icon" :src="game_icon_src" />
                <el-input v-model="game_name" placeholder="请输入游戏名（必须）" class="game-name">
                    <template #prepend>
                        游戏名称
                    </template>
                </el-input>
                <el-input v-model="game_path" placeholder="请选择游戏启动文件（非必须）" class="game-path">
                    <template #prepend>
                        启动文件
                    </template>
                    <template #append>
                        <el-button @click="choose_executable_file()">
                            <el-icon>
                                <document-add />
                            </el-icon>
                        </el-button>
                    </template>
                </el-input>
            </div>
            <el-table :data="save_path" class="save-table">
                <el-table-column fixed prop="type" label="类型" width="120" />
                <el-table-column label="控制" width="120">
                    <template #default="scope">
                        <el-button link type="primary" size="small" @click.prevent="deleteRow(scope.$index)">
                            移除
                        </el-button>
                    </template>
                </el-table-column>
                <el-table-column prop="path" label="路径" />
            </el-table>
            <el-button type="primary" @click="addPath">添加存档文件/文件夹</el-button>
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
.save-table {
    margin-top: 20px;
    margin-bottom: 20px;
}

.select-container {
    height: 90%;
    width: 100%;
}

.game-info {
    height: 99%;
    margin-bottom: 20px;
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

.submit-bar {
    justify-content: flex-end;
    height: 10%;
}
</style>