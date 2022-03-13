<template>
	<el-container class="home-container" direction="vertical">
		<h2>你好 世界</h2>
		<div class="describe-container">
			<div class="describe">
				<h3>简洁</h3>
				<p>不添加推广、贴片广告等无用功能，尽量保证界面为功能服务</p>
			</div>
			<div class="describe">
				<h3>安全</h3>
				<p>
					此软件无联机功能，你可以通过坚果云来备份自己的存档，实现“云存档”的功能
				</p>
			</div>
			<div class="describe">
				<h3>自由</h3>
				<p>
					这个软件属于开源世界，你可以轻易访问到它的源代码，有能者可以为它贡献代码
				</p>
			</div>
		</div>
		<el-container class="new" direction="horizontal">
			<el-result title="导入游戏">
				<template #icon>
					<Edit />
				</template>
				<template #extra>
					<el-button type="primary" @click="go_add_game()">跳转</el-button>
				</template>
			</el-result>
			<el-result title="进入存档管理">
				<template #icon>
					<Files />
				</template>
				<template #extra>
					<el-button type="primary" @click="go_game_manage()">提示</el-button>
				</template>
			</el-result>
			<el-result title="开始备份！">
				<template #icon>
					<UploadFilled />
				</template>
				<template #extra>
					<el-button type="primary" @click="go_backup()">提示</el-button>
				</template>
			</el-result>
		</el-container>
	</el-container>
</template>

<script lang="ts" setup>
import { Edit, UploadFilled, Files } from "@element-plus/icons-vue";
</script>

<script lang="ts">
import { defineComponent } from "vue";
import { ElNotification } from "element-plus";

export default defineComponent({
	components: {},
	data() {
		return {};
	},
	methods: {
		go_add_game() {
			this.$router.push("/add-game");
		},
		go_game_manage(){
			ElNotification({
				type:"info",
				message:"请单击左侧\"存档管理\""
			})
		},
		go_backup(){
			ElNotification({
				type:"info",
				message:"请在\"存档管理\"栏目下单击游戏名，在新界面中进行存档管理"
			})
		}
	},
	computed: {},
});
</script>

<style>
.home-container {
	height: 95%;
}
.home-container > h2 {
	margin-right: auto;
	margin-left: auto;
	margin-bottom: 2em;
	display: block;
	font-size: 2em;
}
.describe-container {
	display: flex;
	justify-content: space-around;
}
.describe {
	width: 25%;
}
.new {
	margin-top: 100px;
	justify-content: space-around;
}
</style>