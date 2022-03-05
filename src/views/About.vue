<template>
	<el-container>
		<el-main class="about-content">
			<h2>该软件是一个开源的，由Sworld主导的游戏存档管理软件</h2>
			<p>
				设计该软件的初衷是为了方便玩家们管理自己的游戏存档，但是由于游戏数量众多，难以一一适配，
				所以部分游戏能够做到自动识别，而更多的游戏只能靠手动导入，如果你感兴趣的话，可以联系我以提交已有的游戏存档位置。
			</p>
			<h2>支持我</h2>
			<p>
				你可以点击下方的“开源地址”，在上面为我点一个Star，这是对我很大的鼓励，也可以通过开源平台为这个项目贡献代码和提出问题。
			</p>
			<p>
				如果你是普通用户，可以考虑请我喝杯饮料，我将会把你的昵称放在捐赠名单上以表示感谢。
			</p>
		</el-main>
		<el-footer>
			<el-link
				@click="source_click('https://gitee.com/sworldS/game-save-manager')"
				>开源地址</el-link
			>
			|
			<el-link
				@click="
					source_click('https://gitee.com/sworldS/game-save-manager/releases/')
				"
				>发布地址</el-link
			>
			<span class="version">版本号: {{ $store.state.config.version }}</span>
		</el-footer>
	</el-container>
</template>

<script lang="ts">
import { defineComponent } from "vue";
const { ipcRenderer } = require("electron");

export default defineComponent({
	methods: {
		source_click(url: string) {
			ipcRenderer.send("open_url",url)
		},
	},
});
</script>


<style>
.version {
	font-size: 0.8rem;
	text-align: right;
	float: right;
}
</style>