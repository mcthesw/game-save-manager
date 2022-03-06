<template>
	<div class="manage-container">
		<!-- 下面是顶栏部分 -->
		<div class="manage-top-bar">
			<template v-for="button in top_buttons" :key="button.id">
				<el-button type="primary" round @click="button_handler(button.method)">
					{{ button.text }}
				</el-button>
			</template>
			<el-button type="danger" round @click="del_cur()">
				删除该存档管理
			</el-button>
		</div>
		<!-- 下面是游戏信息 -->
		<div>
		</div>
		<!-- 下面是主体部分 -->
		<el-card class="saves-container">
			<!-- 存档应当用点击展开+内部表格的方式来展示 -->
			<!-- 这里应该有添加新存档按钮，按下后选择标题和描述进行存档 -->
			<!-- 下面是测试用数据，最后需要被替换成v-for生成的时间轴卡片 -->
			<el-table :data="table_data" style="width: 100%">
				<el-table-column label="备份日期" prop="date" width="200px" />
				<el-table-column label="描述" prop="describe" />
				<el-table-column align="right">
					<template #header>
						<!-- 暂时禁止搜索，之后做 -->
						<el-input
							v-model="search"
							size="small"
							placeholder="输入以搜索标签"
							disabled
							clearable
						/>
					</template>
					<template #default="scope">
						<!-- scope.$index和scope.row可以被使用 -->
						<el-button size="small">应用</el-button>
						<el-button size="small" type="danger">删除</el-button>
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
					date: "2022-2-2 22:37",
					describe: "我狂写",
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
		ipcRenderer.send("get_game_backup", {
			game_name: this.$route.params.name,
		});
	},
	methods: {
		load_game(saves) {
			// TODO: 在路由切换后，把当前游戏的信息读取到data的table_data中
			this.game.name = saves.name;
			this.table_data = saves.saves;
		},
		button_handler(func) {
			// 触发按钮绑定的方法
			this[func]();
		},
		create_new_save() {
			// TODO:实现tags
		},
		load_latest_save() {},
		launch_game() {},
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
					// TODO:跳转回主界面
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
	computed: {},
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
	height: 50px;
	padding-right: 10px;
	padding-left: 10px;
	margin: auto;
	margin-bottom: 10px;

	background-color: #409eff;
	display: flex;
	border-radius: 10px;
	align-items: center;
	color: aliceblue;
}
.saves-container {
	margin: auto;
}
</style>