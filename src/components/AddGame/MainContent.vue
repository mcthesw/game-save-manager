<template>
	<el-card class="game-info">
		<div class="top-part">
			<img
				class="game-icon"
				src="https://shadow.elemecdn.com/app/element/hamburger.9cf7b091-55e9-11e9-a976-7f4d0b07eef6.png"
			/>
		</div>
		<div style="padding: 14px" class="input-container">
			<div class="bottom">
				<el-button type="text" class="button">选择游戏图标 </el-button>
				<el-input v-model="game_name" placeholder="请输入游戏名（必须）">
				</el-input>
				<el-input v-model="save_path" placeholder="请选择存档路径（必须）">
					<template #append>
						<el-button>
							<el-icon>
								<document-add />
							</el-icon>
						</el-button>
					</template>
				</el-input>
				<el-input v-model="game_path" placeholder="请选择游戏启动文件">
					<template #append>
						<el-button>
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
import { defineComponent } from "vue";
import { DocumentAdd } from "@element-plus/icons-vue";

export default defineComponent({
	setup() {},
	components: { DocumentAdd },
	data() {
		return {
			game_name: "",
			save_path: "",
			game_path: "",
		};
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