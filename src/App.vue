<script lang="ts" setup>
import MainSideBar from "./components/MainSideBar.vue";
import { ElNotification } from "element-plus";

ElNotification({
	title: "提示",
	message: "这是一个早期测试版本，不能保证稳定性，请谨慎使用",
	type: "warning",
	duration: 3000,
});


</script>

<template>
	<el-container>
		<el-aside width="200px">
			<MainSideBar />
		</el-aside>
		<el-main>
			<router-view v-slot="{ Component }">
				<transition name="fade" mode="out-in">
					<component :is="Component" />
				</transition>
			</router-view>
		</el-main>
	</el-container>
</template>

<style>
@font-face{
    font-family: 'OppoSans';
    src: url('./assets/fonts/OPPOSans-M.ttf') format('truetype');
}
#app {
	font-family:'OppoSans', Helvetica, Arial, sans-serif;
	-webkit-font-smoothing: antialiased;
	-moz-osx-font-smoothing: grayscale;
}

body,
html,
#app {
	margin: 0px;
	height: 100%;
}

.el-container {
	width: 100%;
	height: 100%;
}

.el-aside,
.el-main {
	margin: 0px;
}

.el-aside {
	overflow-x: unset;
}

a {
	text-decoration: none;
}

.fade-enter-from,
.fade-leave-to {
	opacity: 0;
}

.fade-enter-active {
	transition: all 0.3s ease-out;
}

.fade-leave-active {
	transition: all 0.2s ease-in;
}
</style>
