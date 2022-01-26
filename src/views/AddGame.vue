<template>
	<el-container class="AddGame" direction="vertical">
		<MainContent />
	</el-container>
</template>

<script lang="ts">
import { defineComponent } from "vue";
import MainContent from "@/components/AddGame/MainContent.vue";
const { ipcRenderer } = require("electron");

export default defineComponent({
	setup() {},
	components: { MainContent },
});
</script>

<style>
</style>
