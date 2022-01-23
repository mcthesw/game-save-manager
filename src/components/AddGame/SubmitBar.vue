<template>
	<el-container class="submit-bar">
		<el-tooltip
			v-for="button in buttons"
			:key="button.id"
			:content="button.text"
			placement="top"
		>
			<el-button :type="button.type" circle>
				<el-icon>
					<component :is="button.icon" />
				</el-icon>
			</el-button>
		</el-tooltip>
	</el-container>
</template>

<script lang="ts">
import { defineComponent } from "vue";
import {
	Check,
	RefreshRight,
	Download,
	MagicStick,
} from "@element-plus/icons-vue";

export default defineComponent({
	setup() {},
	components: { Check, RefreshRight, Download, MagicStick },
	data() {
		return {
			buttons: [
				{ text: "修改已有配置", type: null, icon: "MagicStick" },
				{ text: "导入配置合集", type: "primary", icon: "Download" },
				{ text: "保存配置", type: "success", icon: "Check" },
				{ text: "重置当前配置", type: "danger", icon: "RefreshRight" },
			],
		};
	},
});
</script>

<style>
.submit-bar {
	justify-content: flex-end;
}
</style>
