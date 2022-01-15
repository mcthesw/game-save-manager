<template>
	<el-header>
		<h1>关于此软件</h1>
	</el-header>
	<el-main>
		这个软件是一个开源的，由Sworld主导的游戏存档管理软件。
	</el-main>
	<el-footer>
		<el-link @click="source_click('https://gitee.com/sworldS/game-save-manager')">开源地址</el-link> |
		<el-link @click="source_click('https://gitee.com/sworldS/game-save-manager/releases/')">发布地址</el-link>
		<span class="version">版本号: {{$store.state.version}}</span>
	</el-footer>
</template>

<script lang="ts">
import { defineComponent } from 'vue'
const { shell } = window.require('electron');

export default defineComponent({
	setup() {

	},
	methods: {
		source_click(href: string) { shell.openExternal(href) }
	}
})
</script>


<style>
.version {
	font-size: 0.8rem;
	text-align: right;
	float: right;
}
</style>