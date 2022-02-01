<template>
	<div class="home-container">
		<!-- 下面是顶栏部分 -->
		<div class="home-top-bar">
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
					<el-card>
						<h4>和 Tom 打了一半</h4>
						<p>2018/4/3 20:46 打的狗屎对局</p>
					</el-card>
				</el-timeline-item>
				<el-timeline-item timestamp="2018/4/12" placement="top">
					<el-card>
						<h4>快把 Tom 打死了</h4>
						<p>2018/4/12 20:46 麻了，还没打完</p>
					</el-card>
				</el-timeline-item>
				<el-timeline-item timestamp="2018/4/14" placement="top">
					<el-card>
						<h4> Tom 即将投降</h4>
						<p>2018/4/12 20:46 打完了</p>
					</el-card>
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
				{ text: "黑魂3", method: "" },
				{ text: "帝国4", method: "" },
				{ text: "星露谷物语", method: "" },
				{ text: "只狼", method: "" },
				{ text: "神必海域", method: "" },
			],
		};
	},
	methods: {},
});
</script>

<style>
.home-top-bar {
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