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
				<p>此软件无联机功能，你可以通过坚果云来备份自己的存档，实现“云存档”的功能</p>
			</div>
			<div class="describe">
				<h3>自由</h3>
				<p>这个软件属于开源世界，你可以轻易访问到它的源代码，有能者可以为它贡献代码</p>
			</div>
		</div>
		<div class="new">
			<p>第一次使用？不妨按照以下步骤来操作</p>
			<el-steps :space="200" :active="3" simple>
				<el-step title="自动识别存档" :icon="Edit"></el-step>
				<el-step title="进入存档管理" :icon="Files"></el-step>
				<el-step title="开始备份！" :icon="UploadFilled"></el-step>
			</el-steps>
		</div>
	</el-container>
</template>

<script lang="ts" setup>
import { Edit, UploadFilled, Files } from "@element-plus/icons-vue";
</script>

<script lang="ts">
import { defineComponent } from "vue";

export default defineComponent({
	components: {},
	data() {
		return {};
	},
	methods: {},
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
.describe-container{
	display: flex;
	justify-content: space-around;
}
.describe {
	width: 25%;
}
.new {
	margin-top: 200px;
}
</style>