<template>
	<el-menu class="main-side-bar" router default-active="/home">
		<el-scrollbar>
			<!-- 下方是存档栏 -->
			<el-sub-menu index="/management">
				<template #title>
					<el-icon><Files></Files></el-icon>
					<span>存档管理</span>
				</template>
				<el-menu-item
					v-for="save in save_file"
					:key="save.id"
					:index="'/management/' + save.name"
				>
					{{ save.name }}
				</el-menu-item>
			</el-sub-menu>
			<!-- 下方是常规按钮 -->
			<el-menu-item v-for="link in links" :index="link.link" :key="link.key">
				<el-icon> <component :is="link.icon"></component> </el-icon>
				<span>{{ link.text }}</span>
			</el-menu-item>
		</el-scrollbar>
	</el-menu>
</template>

<script lang="ts">
import { defineComponent } from "vue";
import {
	DocumentAdd,
	Files,
	Promotion,
	InfoFilled,
	HotWater,
	Setting
} from "@element-plus/icons-vue";
import { store } from "@/store";

export default defineComponent({
	components: { DocumentAdd, Files, Promotion, InfoFilled, HotWater, Setting},
	data() {
		return {
			links: [
				{ text: "欢迎界面", link: "/home", icon: "HotWater" },
				{ text: "添加游戏", link: "/add-game", icon: "DocumentAdd" },
				{ text: "设置", link: "/settings", icon: "Setting" },
				{ text: "关于", link: "/about", icon: "InfoFilled" },
			],
		};
	},
	computed: {
		save_file() {
			// TODO:读取json文件，通过commit放入store
			return store.state.save_file.games.default;
		},
	},
});
</script>

<style>
.main-side-bar {
	height: 100%;
}
</style>