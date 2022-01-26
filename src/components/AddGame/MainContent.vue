<template>
	<el-card class="game-info">
		<div class="top-part">
			<img class="game-icon" :src="game_icon_src" />
		</div>
		<div style="padding: 14px" class="input-container">
			<div class="bottom">
				<el-button @click="choose_game_icon()" type="text" class="button"
					>选择游戏图标
				</el-button>
				<el-input v-model="game_name" placeholder="请输入游戏名（必须）" />
				<el-input v-model="save_path" placeholder="请选择存档路径（必须）">
					<template #append>
						<el-button @click="choose_save_directory()">
							<el-icon>
								<document-add />
							</el-icon>
						</el-button>
					</template>
				</el-input>
				<el-input v-model="game_path" placeholder="请选择游戏启动文件">
					<template #append>
						<el-button @click="choose_executable_file()">
							<el-icon>
								<document-add />
							</el-icon>
						</el-button>
					</template>
				</el-input>
			</div>
		</div>
	</el-card>
</template>

<script lang="ts">
import { defineComponent, onMounted } from "vue";
import { DocumentAdd } from "@element-plus/icons-vue";
import { ElNotification } from "element-plus";
const { ipcRenderer } = require("electron");

export default defineComponent({
	mounted() {
		ipcRenderer.on("choose_save_directory_reply", (Event, arg) => {
			// 选择游戏存档目录
			this.save_path = arg.filePaths[0] ? arg.filePaths[0] : this.save_path;
		});
		ipcRenderer.on("choose_executable_file_reply", (Event, arg) => {
			// 选择游戏存档目录
			this.game_path = arg.filePaths[0] ? arg.filePaths[0] : this.game_path;
		});
		ipcRenderer.on("choose_game_icon_reply", (Event, arg) => {
			// 选择游戏图标地址
			this.game_icon_src = arg;
		});
	},
	components: { DocumentAdd },
	data() {
		return {
			game_name: "", // 写入游戏名
			save_path: "", // 选择游戏存档目录
			game_path: "", // 选择游戏启动程序
			game_icon_src:
				"https://shadow.elemecdn.com/app/element/hamburger.9cf7b091-55e9-11e9-a976-7f4d0b07eef6.png",
		};
	},
	methods: {
		choose_save_directory() {
			// 选择游戏存档目录
			ipcRenderer.send("choose_save_directory");
		},
		choose_executable_file() {
			// 选择游戏启动程序
			ipcRenderer.send("choose_executable_file");
		},
		choose_game_icon() {
			// 选择游戏图标地址
			ipcRenderer.send("choose_game_icon");
		},
		import_config() {
			// 导入已有配置
		},
		save_config() {
			// 保存当前配置
		},
		reset_config() {
			// 重置当前配置
			this.game_name = "";
			this.save_path = "";
			this.game_path = "";
			this.game_icon_src =
				"https://shadow.elemecdn.com/app/element/hamburger.9cf7b091-55e9-11e9-a976-7f4d0b07eef6.png";
			ElNotification({
				title: "提示",
				message: "重置成功",
				type: "success",
				duration: 1000,
			});
		},
	},
});
</script>

<style>
.game-info {
	height: 99%;
}
.top-part {
	height: 200px;
}
.game-icon {
	float: left;
	height: 200px;
	width: 200px;
}
.el-input {
	margin-top: 10px;
}
</style>