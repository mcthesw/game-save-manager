<template>
	<el-menu
		router
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
</template>

<script lang="ts">
import { defineComponent } from 'vue'
import { DocumentAdd, Files, Promotion, InfoFilled } from '@element-plus/icons-vue'

export default defineComponent({
	components: { DocumentAdd, Files, Promotion, InfoFilled, },
    data(){
        return{
            links : [
                {text:"存档管理",link:"/management"},
                {text:"导入导出",link:"/in-out"},
                {text:"添加游戏",link:"/add-game"},
                {text:"关于",link:"/about"},
            ]
        }
    }
})
</script>

<style>
</style>