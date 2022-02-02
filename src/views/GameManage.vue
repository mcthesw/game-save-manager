<template>
	<div class="manage-container">
		<!-- 下面是顶栏部分 -->
		<div class="manage-top-bar">
			<template v-for="button in top_buttons" :key="button.id">
				<el-button type="primary" round>
					{{ button.text }}
				</el-button>
			</template>
		</div>
		<!-- 下面是主体部分 -->
		<el-card class="saves-container">
			<!-- 存档应当用点击展开+内部时间线的方式来展示 -->
			<!-- 这里应该有添加新存档按钮，按下后选择标题和描述进行存档 -->
			<!-- 下面是测试用数据，最后需要被替换成v-for生成的时间轴卡片 -->
			<el-timeline>
				<el-timeline-item center timestamp="2018/4/3" placement="top">
					<h4>和 Tom 打了一半</h4>
					<p>2018/4/3 20:46 打的狗屎对局</p>
				</el-timeline-item>
			</el-timeline>
		</el-card>
	</div>
</template>

<script lang="ts">
import { defineComponent } from "vue";

export default defineComponent({
	components: {},
	data() {
		return {
			top_buttons: [
				{ text: "创建新存档", method: "" },
				{ text: "装载最新的存档", method: "" },
				{ text: "启动游戏", method: "" },
			],
		};
	},
	methods: {
		load_game(name:string){
		}
	},
	computed: {},
	created() {
		this.$watch(
			// TODO:需要根据路由来切换游戏
			() => this.$route.params,
			(newVal: any, oldVal: any) => {
				if(newVal == {}){return}
				console.log(oldVal, "-->", newVal);
				this.load_game(newVal.name)
			}
		);
	},
});
</script>

<style>
.manage-top-bar {
	width: 98%;
	height: 50px;
	padding-right: 10px;
	padding-left: 10px;
	margin: auto;
	margin-bottom: 10px;

	background-color: #409eff;
	display: flex;
	border-radius: 10px;
	align-items: center;
	color: aliceblue;
}
.saves-container {
	margin: auto;
}
</style>