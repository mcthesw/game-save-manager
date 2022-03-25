<template>
	<el-container class="setting" direction="vertical">
		<el-card>
			<h1>个性化设置</h1>
			<el-button @click="submit_config()">保存修改</el-button>
			<el-button @click="abort_change()">放弃修改</el-button>
			<el-button @click="reset_settings()" type="danger"
				>恢复默认配置</el-button
			>
			<br />
			<div class="setting-box">
				<ElSwitch
					v-model="config.settings.prompt_when_not_described"
					:loading="loading"
				/>
				<span>当未描述存档时，弹出提示</span>
			</div>
			<div class="setting-box">
				<ElSwitch
					v-model="config.settings.extra_backup_when_apply"
					:loading="loading"
				/>
				<span>在应用存档时进行额外备份（在 ./save_data/游戏名/extra_backup 文件夹内）</span>
			</div>
		</el-card>
	</el-container>
</template>

<script lang="ts">
import { defineComponent } from "vue";
import { Settings, Config } from "../background/saveTypes";
import { store } from "@/store";
import { default_config } from "../background/config";
import { ElNotification } from "element-plus";
import { ipcRenderer } from "electron";
import { toRaw } from "@vue/reactivity";

export default defineComponent({
	mounted() {
		this.load_config();
		ipcRenderer.on("reply_set_config", (Event, arg) => {
			this.load_config();
			let message: string;
			let type: "success" | "error";
			if (arg) {
				message = "更改成功";
				type = "success";
			} else {
				message = "更改失败";
				type = "error";
			}
			ElNotification({
				message: message,
				type: type,
			});
			this.loading = false;
		});
	},
	beforeUnmount() {
		ipcRenderer.removeAllListeners("reply_set_config");
	},
	components: {},
	data(): { config: Config; loading: boolean } {
		return {
			config: JSON.parse(JSON.stringify(default_config)),
			loading: false,
		};
	},
	methods: {
		load_config() {
			ipcRenderer.send("get_config");
			setTimeout(this.use_config, 100);
		},
		submit_config() {
			this.loading = true;
			ipcRenderer.send("set_config", toRaw(this.config));
		},
		abort_change() {
			ElNotification({
				type: "success",
				message: "重置成功",
			});
			this.load_config();
		},
		reset_settings() {
			this.config.settings = JSON.parse(
				JSON.stringify(default_config)
			).settings;
			ElNotification({
				type: "warning",
				message: "注意，保存后该重置才会生效",
			});
		},
		use_config() {
			this.config = JSON.parse(JSON.stringify(store.state.config));
			console.log("获取到了软件设置", this.config.settings);
		},
	},
});
</script>

<style>
.el-switch {
	margin-right: 20px;
}
.setting-box {
	margin-top: 10px;
}
</style>
