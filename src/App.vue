<script lang="ts" setup>
import MainSideBar from "./components/MainSideBar.vue";
import { show_error, show_info, show_warning } from "./utils/notifications"
import { useConfig } from "./stores/ConfigFile";
import { invoke } from "@tauri-apps/api/tauri";
import { listen } from "@tauri-apps/api/event"
import { IpcNotification, EventWrapper } from "./schemas/events";
import { useDark } from '@vueuse/core'

// load dark mode status
useDark()

// load config
let config = useConfig();
invoke("local_config_check").then((x) => {
	config.refresh(); // TODO:Handle old version config
}).catch((e) => {
	console.log(e)
});

show_warning("这是一个早期测试版本，不能保证稳定性，请谨慎使用");

listen('Notification', (event: unknown) => {
	let ev = (event as EventWrapper<IpcNotification>).payload
	switch (ev.level) {
		case "info": show_info(ev.msg, ev.title); break;
		case "warning": show_warning(ev.msg, ev.title); break;
		case "error": show_error(ev.msg, ev.title); break;
	}
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
#app {
	font-family: -apple-system, BlinkMacSystemFont, Segoe UI, Helvetica, Arial, sans-serif, Apple Color Emoji, Segoe UI Emoji;
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
