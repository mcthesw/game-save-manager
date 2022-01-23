<template>
	<el-container class="AddGame">
		<el-main>
			<MainContent />
		</el-main>
		<el-footer>
			<SubmitBar />
		</el-footer>
	</el-container>
</template>

<script lang="ts">
import { defineComponent } from "vue";
import SubmitBar from "@/components/AddGame/SubmitBar.vue";
import MainContent from "@/components/AddGame/MainContent.vue";
const { ipcRenderer } = require("electron");

export default defineComponent({
	setup() {},
	components: { SubmitBar, MainContent },
});
</script>

<style>
</style>
