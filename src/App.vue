<script lang="ts" setup>
import MainSideBar from "./components/MainSideBar.vue";
import { show_error, show_info, show_warning } from "./utils/notifications"
import { useConfig } from "./stores/ConfigFile";
import { listen } from "@tauri-apps/api/event"
import { IpcNotification, EventWrapper } from "./schemas/events";
import { useDark } from '@vueuse/core'
import { $t } from "./i18n";
import { useRouter } from "vue-router";

// load dark mode status
useDark()
// load config
const config = useConfig();
const router = useRouter();
config.refresh().then(() => {
	// load home page
	router.push(config.settings.home_page).catch(() => {
		show_error($t("home.wrong_homepage"))
		router.push("/home")
	})
})



// show_warning($t('app.early_access_warning'));

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
