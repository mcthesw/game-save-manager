<script lang="ts" setup>
//TODO: Add error handler
import { Ref, computed, ref, watch } from "vue";
import { ElMessageBox } from "element-plus";
import { invoke } from "@tauri-apps/api/tauri";
import { useConfig } from "../stores/ConfigFile";
import { BackupsInfo, Game } from "../schemas/saveTypes";
import { useRoute, useRouter } from "vue-router";
import { show_error, show_info, show_success, show_warning } from "../utils/notifications";

let config = useConfig();
let router = useRouter();
let route = useRoute();
const top_buttons = [
    { text: "创建新存档", method: create_new_save },
    { text: "用最新存档覆盖", method: load_latest_save },
    { text: "启动游戏", method: launch_game },
    { text: "打开备份文件夹", method: open_backup_folder },
    { text: "修改存档管理", method: edit_cur}
]

const search = ref("");

let table_data = ref([
    {
        date: "",
        describe: "这是一条错误信息，正常情况不会出现",
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

// Init game info
watch(
    () => route.params,
    (newValue) => {
        if (!newValue.name) { return; }
        let name = newValue.name;
        console.log("Current game:", name)
        game.value = config.games.find((x) => x.name == name) as Game;
        console.log(game.value)
        refresh_backups_info()
    },
    { immediate: true }
)

function refresh_backups_info() {
    invoke("get_backups_info", { game: game.value })
        .then((v) => {
            let infos = v as BackupsInfo;
            table_data.value = infos.backups;
            console.log(v)
        }).catch(
            (e) => { console.log(e) }
        )
}

function send_save_to_background() {
    show_info("当该游戏存档大时操作会很久，请等提示成功后再进行其他操作");
    if (!backup_button_time_limit) {
        show_error("无法在一秒内进行多次存档");
        return;
    }
    if (!backup_button_backup_limit) {
        show_error("上次备份还未完成，请等待");
        return;
    }
    if (!apply_button_apply_limit) {
        show_error("上次覆盖还未完成，请等待");
        return;
    }
    backup_button_time_limit = false;
    backup_button_backup_limit = false;
    invoke("backup_save", { game: game.value, describe: describe.value })
        .then((x) => {
            show_success("备份成功");
        }).catch(
            (e) => { console.log(e) }
        ).finally(() => {
            backup_button_backup_limit = true
            refresh_backups_info();
        })

    describe.value == "";
    setTimeout(() => {
        backup_button_time_limit = true;
    }, 1000);
}

function create_new_save() {
    if (
        config.settings.prompt_when_not_described && !describe.value
    ) {
        ElMessageBox.confirm("你没有给这个存档提供描述，继续吗？", "警告", {
            confirmButtonText: "坚持保存",
            cancelButtonText: "取消",
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
        show_error("您并没有储存过该游戏的启动方式");
        return;
    } else {
        invoke("open_url", { url: game.value.game_path })
            .then((x) => {
                console.log(x)
            }).catch(
                (e) => { console.log(e) }
            )
    }
}

function del_save(date: string) {
    console.log(date)
    invoke("delete_backup", { game: game.value, date: date }).then((x) => {
        console.log(x)
        refresh_backups_info();
        show_success("删除成功");
    }).catch(
        (e) => { console.log(e) }
    )
}

function apply_save(date: string) {
    show_warning("当该游戏存档大时操作会很久，请等提示成功后再进行其他操作");

    if (!apply_button_apply_limit) {
        show_error("上次覆盖还未完成，请等待");
        return;
    }
    if (!backup_button_backup_limit) {
        show_error("上次备份还未完成，请等待")
        return;
    }
    apply_button_apply_limit = false;
    invoke("apply_backup", { game: game.value, date: date })
        .then((x) => {
            show_success("恢复成功");
            console.log(x)
        }).catch((e) => {
            console.log(e)
        }).finally(() => {
            apply_button_apply_limit = true;
            refresh_backups_info();
        })
}

function load_latest_save() {
    if (table_data.value[0].date) {
        apply_save(table_data.value[0].date);
    } else {
        show_error("发生了错误，可能您没有任何存档");
    }
}

function del_cur() {
    ElMessageBox.prompt(
        "如果确定删除的话，请输入yes，否则请点击取消。这个操作将会抹除已经备份过的该游戏的所有存档，并且把该游戏从已识别列表中去除",
        "提示",
        {
            confirmButtonText: "确定",
            cancelButtonText: "取消",
            inputPattern: /yes/,
            inputErrorMessage: "无效的输入",
        }
    )
        .then(() => {
            invoke("delete_game", { game: game.value })
            setTimeout(() => {
                config.refresh()
                router.back()
            }, 100)
        })
        .catch(() => {
            show_info("您取消了这次操作");
        });
}

function open_backup_folder() {
    invoke("open_backup_folder", { game: game.value })
        .then((x) => {
            console.log(x)
        }).catch(
            (e) => { console.log(e) }
        )
}

// 点击按钮后，，跳转到添加游戏页面
function edit_cur() {
    config.refresh()
    router.push({
        name: "add-game",
        params: {
            name: game.value.name,
        },
    });
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
            <template v-for="button in top_buttons" :key="button.id">
                <el-button type="primary" round @click="button.method">
                    {{ button.text }}
                </el-button>
            </template>
            <el-button type="danger" round @click="del_cur()">
                删除该存档管理
            </el-button>

            <!-- 下面是当前存档描述信息 -->

            <el-input v-model="describe" placeholder="请输入新存档描述信息">
                <template #prepend>{{ game.name }}的新存档: </template>
            </el-input>
        </el-card>
        <!-- 下面是主体部分 -->
        <el-card class="saves-container">
            <!-- 存档应当用点击展开+内部表格的方式来展示 -->
            <!-- 这里应该有添加新存档按钮，按下后选择标题和描述进行存档 -->
            <!-- 下面是测试用数据，最后需要被替换成v-for生成的时间轴卡片 -->
            <el-table :data="filter_table" style="width: 100%">
                <el-table-column label="备份日期" prop="date" width="200px" sortable />
                <el-table-column label="描述" prop="describe" />
                <el-table-column align="right">
                    <template #header>
                        <!-- 搜索 -->
                        <el-input v-model="search" size="small" placeholder="输入以搜索描述" clearable />
                    </template>
                    <template #default="scope">
                        <!-- scope.$index和scope.row可以被使用 -->
                        <el-popconfirm title="确定覆盖现有存档?" @confirm="apply_save(scope.row.date)">
                            <template #reference>
                                <el-button size="small"> 应用 </el-button>
                            </template>
                        </el-popconfirm>
                        <el-popconfirm title="确定要删除?" @confirm="del_save(scope.row.date)">
                            <template #reference>
                                <el-button size="small" type="danger">
                                    删除
                                </el-button></template>
                        </el-popconfirm>
                    </template>
                </el-table-column>
            </el-table>
        </el-card>
    </div>
</template>

<style scoped>
.manage-top-bar {
    width: 98%;
    padding-right: 10px;
    padding-left: 10px;
    margin: auto;
    margin-bottom: 5px;

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