<template>
	<div class="manage-container">
		<!-- 下面是顶栏部分 -->
		<el-card class="manage-top-bar">
			<template v-for="button in top_buttons" :key="button.id">
				<el-button type="primary" round @click="button_handler(button.method)">
					{{ button.text }}
				</el-button>
			</template>
			<el-button type="danger" round @click="del_cur()">
				删除该存档管理
			</el-button>

			<!-- 下面是当前存档描述信息 -->

			<el-input v-model="describe" placeholder="请输入新存档描述信息">
				<template #prepend>{{ this.game.name }}的新存档: </template>
			</el-input>
		</el-card>
		<!-- 下面是主体部分 -->
		<el-card class="saves-container">
			<!-- 存档应当用点击展开+内部表格的方式来展示 -->
			<!-- 这里应该有添加新存档按钮，按下后选择标题和描述进行存档 -->
			<!-- 下面是测试用数据，最后需要被替换成v-for生成的时间轴卡片 -->
			<el-table :data="filter_table" style="width: 100%">
				<el-table-column label="备份日期" prop="date" width="200px" />
				<el-table-column label="描述" prop="describe" />
				<el-table-column align="right">
					<template #header>
						<!-- 暂时禁止搜索，之后做 -->
						<el-input
							v-model="search"
							size="small"
							placeholder="输入以搜索描述"
							clearable
						/>
					</template>
					<template #default="scope">
						<!-- scope.$index和scope.row可以被使用 -->
						<el-button size="small" @click="apply_save(scope.row.date)">
							应用
						</el-button>
						<el-button
							size="small"
							type="danger"
							@click="del_save(scope.row.date)"
						>
							删除
						</el-button>
					</template>
				</el-table-column>
			</el-table>
		</el-card>
	</div>
</template>

<script>
// !这里不使用lang = 'ts'是为了保证button_handler运行
import { defineComponent } from "vue";
import { ElMessageBox, ElNotification } from "element-plus";
import { ipcRenderer } from "electron";
import { Config, Game, Saves, Save } from "../background/saveTypes";

export default defineComponent({
	components: {},
	data() {
		return {
			top_buttons: [
				{ text: "创建新存档", method: "create_new_save" },
				{ text: "装载最新的存档", method: "load_latest_save" },
				{ text: "启动游戏", method: "launch_game" },
			],
			search: "",
			table_data: [
				{
					date: "",
					describe: "这是一条错误信息，正常情况不会出现",
					tags: [],
					path: "",
				},
			],
			game: {
				name: "",
				save_path: "",
				game_path: "",
				icon: "",
			},
			describe: "",
		};
	},
	mounted() {
		ipcRenderer.on("reply_get_game_backup", (Event, arg) => {
			this.load_game(arg);
		});
		ipcRenderer.on("reply_delete_game", (Event, arg) => {
			if (!arg) {
				ElNotification({
					type: "warning",
					message: "删除失败",
				});
				return;
			}
			ElNotification({
				type: "info",
				message: "您删除了该存档管理",
			});
			this.$router.push("/home");
			ipcRenderer.send("get_config");
		});
		ipcRenderer.on("reply_backup", (Event, arg) => {
			let type;
			let message;
			if (arg) {
				type = "success";
				message = "备份成功";
			} else {
				type = "error";
				message = "备份失败";
			}
			ElNotification({
				type: type,
				message: message,
			});
			ipcRenderer.send("get_game_backup", {
				game_name: this.$route.params.name,
			});
		});
		ipcRenderer.on("reply_delete_save", (Event, arg) => {
			let type;
			let message;
			if (arg) {
				type = "success";
				message = "删除成功";
			} else {
				type = "error";
				message = "删除失败";
			}
			ElNotification({
				type: type,
				message: message,
			});
			ipcRenderer.send("get_game_backup", {
				game_name: this.$route.params.name,
			});
		});
		ipcRenderer.on("reply_apply_backup", (Event, arg) => {
			let type;
			let message;
			if (arg) {
				type = "success";
				message = "恢复成功";
			} else {
				type = "error";
				message = "恢复失败";
			}
			ElNotification({
				type: type,
				message: message,
			});
		});

		ipcRenderer.send("get_game_backup", {
			game_name: this.$route.params.name,
		});
	},
	methods: {
		load_game(saves) {
			// 在路由切换后，把当前游戏的信息读取到data的table_data中
			this.game.name = saves.name;
			this.table_data = saves.saves;
		},
		button_handler(func) {
			// 触发按钮绑定的方法
			this[func]();
		},
		create_new_save() {
			// TODO:实现tags或者删除tags
			ipcRenderer.send("backup", {
				game_name: this.game.name,
				describe: this.describe,
				tags: [],
			});
			this.describe == "";
		},
		load_latest_save() {
			if(this.table_data[0].date){
				this.apply_save(this.table_data[0].date)
			}else{
				ElNotification({
					type: "error",
					message: "发生了错误，可能您没有任何存档",
				});
			}
			
		},
		launch_game() {
			if (this.game.game_path.length < 4) {
				ElNotification({
					type: "error",
					message: "您并没有储存过该游戏的启动方式",
				});
				return;
			} else {
				ipcRenderer.send("open_url", this.game.game_path);
			}
		},
		del_save(date) {
			ipcRenderer.send("delete_save", {
				game_name: this.game.name,
				save_date: date,
			});
		},
		apply_save(date) {
			ipcRenderer.send("apply_backup", {
				game_name: this.game.name,
				save_date: date,
			});
		},
		del_cur() {
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
					ipcRenderer.send("delete_game", { game_name: this.game.name });
				})
				.catch(() => {
					ElNotification({
						type: "info",
						message: "您取消了这次操作",
					});
				});
		},
	},
	computed: {
		filter_table() {
			return this.table_data.filter(
				(data) => !this.search || data.describe.includes(this.search)
			);
		},
	},
	created() {
		this.$watch(
			// TODO:需要根据路由来切换游戏
			() => this.$route.params,
			(newVal, oldVal) => {
				console.log("选中游戏", newVal.name);
				if (!newVal.name) {
					return;
				}
				ipcRenderer.send("get_game_backup", {
					game_name: newVal.name,
				});
			}
		);
	},
});
</script>

<style>
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
.saves-container {
	margin: auto;
}
</style>