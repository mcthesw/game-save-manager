<template>
	<el-menu class="main-side-bar" router default-active="/management">
		<!-- 下方是存档栏 -->
		<el-sub-menu index="0">
			<template #title>
				<el-icon><Files></Files></el-icon>
				<span>存档管理</span>
			</template>
			<el-menu-item
				v-for="save in save_file.saves.default"
				:key="save.id"
				:index="'/management/' + save.name"
			>
				{{ save.name }}
			</el-menu-item>
		</el-sub-menu>
		<!-- 下方是常规按钮 -->
		<el-menu-item v-for="link in links" :index="link.link" :key="link.key">
			<el-icon> <component :is="link.icon"></component> </el-icon
			>{{ link.text }}
		</el-menu-item>
	</el-menu>
</template>

<script lang="ts">
import { defineComponent } from "vue";
import {
	DocumentAdd,
	Files,
	Promotion,
	InfoFilled,
} from "@element-plus/icons-vue";
import store from "@/store";

export default defineComponent({
	components: { DocumentAdd, Files, Promotion, InfoFilled },
	data() {
		return {
			links: [
				{ text: "存档管理(临时)", link: "/management", icon: "Files" },
				{ text: "添加游戏", link: "/add-game", icon: "DocumentAdd" },
				{ text: "关于", link: "/about", icon: "InfoFilled" },
			],
		};
	},
	computed: {
		save_file() {
			// TODO:读取json文件，通过commit放入store
			return {
				version: "V0",
				saves: {
					default: [{ name: "黑魂3" }, { name: "星露谷物语" }],
					custom: [],
				},
			};
		},
	},
});
</script>

<style>
.main-side-bar{
	height: 100%;
}
</style>