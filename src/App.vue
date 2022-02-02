<template>
	<el-container>
		<el-aside width="200px">
			<MainSideBar />
		</el-aside>
		<el-main>
			<transition name="fade" mode="in-out">
				<router-view />
			</transition>
		</el-main>
	</el-container>
</template>

<script lang="ts">
import { defineComponent } from "vue";
import MainSideBar from "./components/MainSideBar.vue";
import { ElNotification } from "element-plus";
import { store } from "./store";

export default defineComponent({
	name: "App",
	components: { MainSideBar },
	mounted() {
		// 提示这是早期版本
		ElNotification({
			title: "提示",
			message: "这是一个早期测试版本，不能保证稳定性，请谨慎使用",
			type: "warning",
			duration: 3000,
		});
		this.get_saved_games();
	},
	methods: {
		get_saved_games() {
			store.dispatch("get_saved_games");
		},
	},
});
</script>

<style>
#app {
	font-family: Avenir, Helvetica, Arial, sans-serif;
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
.el-aside{
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
	transition: all 0.8s ease-out;
}

.fade-leave-active {
	transition: all 0.4s ease-in;
}
</style>
