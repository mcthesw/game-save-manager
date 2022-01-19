<template>
	<el-container class="AddGame">
		<el-main>这里显示基本信息</el-main>
		<el-footer>
            <SubmitBar />
        </el-footer>
	</el-container>
</template>

<script lang="ts">
import { defineComponent } from 'vue'
import SubmitBar from '@/components/AddGame/SubmitBar.vue'
const { ipcRenderer } = require('electron')

export default defineComponent({
	setup() { },
    components:{SubmitBar}
})
</script>

<style>
</style>
