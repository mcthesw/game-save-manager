<template>
	<el-container class="change-game-info">
		<el-main>
			<el-scrollbar>
				<el-card v-for="game in Object.keys(this.games)" :key="game.id">
					<el-tag>游戏名：{{ game }}</el-tag>
					<el-input v-model="games[game].save_path">
						<template #prepend>存档目录</template>
					</el-input>
					<el-input v-model="games[game].game_path">
						<template #prepend>启动文件</template>
					</el-input>
				</el-card>
			</el-scrollbar>
		</el-main>
		<el-footer>
			<el-button @click="commit_change()">保存修改</el-button>
			<el-button @click="abort_changes()">放弃修改</el-button>
			<el-button type="primary" @click="back()">返回</el-button>
		</el-footer>
	</el-container>
</template>

<script lang="ts">
import { defineComponent } from "vue";
import router from "@/router";
import { Games } from "../background/saveTypes";
import { store } from "@/store";
import { ElNotification } from "element-plus";
const { ipcRenderer } = require("electron");
import { toRaw } from "@vue/reactivity";

export default defineComponent({
	mounted() {
		this.load_games();

		ipcRenderer.on("reply_set_game_infos", (Event, arg) => {
			this.reload_data();
			if (arg) {
				ElNotification({ type: "success", message: "更新成功" });
			} else {
				ElNotification({
					type: "error",
					message: "后端发生未知错误，保存失败",
				});
			}
		});
	},
	unmounted() {
		ipcRenderer.removeAllListeners("reply_set_game_infos");
	},
	data(): { games: Games } {
		return {
			games: {},
		};
	},
	methods: {
		back() {
			this.reload_data();
			router.push("/add-game");
		},
		reload_data() {
			ipcRenderer.send("get_config");
			this.load_games();
		},
		abort_changes() {
			this.reload_data();
			ElNotification({ type: "success", message: "重置成功" });
		},
		commit_change() {
			if (this.games == {} || !this.games) {
				ElNotification({
					type: "error",
					message: "发生未知错误,保存失败,可能是输入的问题",
				});
				return;
			}
			ipcRenderer.send("set_game_infos", toRaw(this.games));
		},
		load_games() {
			setTimeout(() => {
				this.games = store.state.config.games;
			}, 50);
		},
	},
});
</script>

<style>
.change-game-info .el-footer {
	display: flex;
	justify-content: flex-end;
}
.change-game-info .el-card {
	margin-top: 20px;
}
</style>