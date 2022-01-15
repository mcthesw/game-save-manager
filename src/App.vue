<template>
	<el-container>
		<el-aside width="150px">
			<el-menu
				router="true"
				default-active="/management"
			>
				<el-menu-item index="/management">
					<el-icon>
						<files />
					</el-icon>存档管理
				</el-menu-item>
				<el-menu-item index="/in-out">
					<el-icon>
						<promotion />
					</el-icon>导入导出
				</el-menu-item>
				<el-menu-item index="/add-game">
					<el-icon>
						<document-add />
					</el-icon>添加游戏
				</el-menu-item>
				<el-menu-item index="/about">
					<el-icon>
						<info-filled />
					</el-icon>关于
				</el-menu-item>
			</el-menu>
		</el-aside>
		<el-main>
			<transition
				name="fade"
				mode="in-out"
			>
				<router-view />
			</transition>
		</el-main>
	</el-container>
</template>

<script lang="ts">
import { DocumentAdd, Files, Promotion, InfoFilled } from '@element-plus/icons-vue'
import { defineComponent } from 'vue'

export default defineComponent({
	name: 'App',
	components: { DocumentAdd, Files, Promotion, InfoFilled },
})
</script>

<style>
#app {
	font-family: Avenir, Helvetica, Arial, sans-serif;
	-webkit-font-smoothing: antialiased;
	-moz-osx-font-smoothing: grayscale;
}

el-container {
	width: 100%;
	height: 100%;
}
.el-aside,
.el-main {
	height: 100%;
}

a {
	text-decoration: none;
}

.fade-enter-from,
.fade-leave-to {
	opacity: 0;
}

.fade-enter-active {
	transition: all 0.8s ease-out;
}

.fade-leave-active {
	transition: all 0.4s ease-in;
}
</style>
