<template>
	<el-menu
		router
		default-active="/management"
	>
		<el-menu-item
			v-for="link in links"
			:index="link.link"
			:key="link.key"
		>
			<el-icon>
				<component :is="link.icon"></component>
			</el-icon>{{link.text}}
		</el-menu-item>
	</el-menu>
</template>

<script lang="ts">
import { defineComponent } from 'vue'
import { DocumentAdd, Files, Promotion, InfoFilled } from '@element-plus/icons-vue'

export default defineComponent({
	components: { DocumentAdd, Files, Promotion, InfoFilled, },
	data() {
		return {
			links: [
				{ text: "存档管理", link: "/management", icon: "Files" },
				{ text: "导入导出", link: "/in-out", icon: "Promotion" },
				{ text: "添加游戏", link: "/add-game", icon: "DocumentAdd" },
				{ text: "关于", link: "/about", icon: "InfoFilled" },
			]
		}
	}
})
</script>

<style>
</style>