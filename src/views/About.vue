<template>
	<el-container>
		<el-header>
			<h1>关于此软件</h1>
		</el-header>
		<el-main class="about-content">
			<p>这个软件是一个开源的，由Sworld主导的游戏存档管理软件。</p>
			<p>
				设计该软件的初衷是为了方便玩家们管理自己的游戏存档，但是由于游戏数量众多，难以一一适配，
				所以部分游戏能够做到自动识别，而更多的游戏只能靠手动导入，如果你感兴趣的话，可以联系我以提交已有的游戏存档位置。
			</p>
		</el-main>
		<el-footer>
			<el-link @click="source_click('https://gitee.com/sworldS/game-save-manager')">开源地址</el-link> |
			<el-link @click="source_click('https://gitee.com/sworldS/game-save-manager/releases/')">发布地址</el-link>
			<span class="version">版本号: {{$store.state.version}}</span>
		</el-footer>
	</el-container>
</template>

<script lang="ts">
import { defineComponent } from 'vue'
const { shell } = window.require('electron');

export default defineComponent({
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