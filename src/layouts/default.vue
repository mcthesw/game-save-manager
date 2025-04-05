<script setup lang="ts">
import { ref, provide } from 'vue';

// 创建一个响应式的侧边栏宽度变量
const sidebarWidth = ref(240);

// 提供给子组件使用
provide('sidebarWidth', sidebarWidth);
</script>

<template>
    <ElContainer>
        <ElAside :width="sidebarWidth + 'px'">
            <MainSideBar />
        </ElAside>
        <ElScrollbar>
            <ElMain>
                <slot />
            </ElMain>
        </ElScrollbar>
    </ElContainer>
</template>

<style scoped>
.el-container {
    height: 100vh;
    overflow: hidden;
}

.el-aside {
    height: 100%;
    overflow: hidden;
}

.el-scrollbar {
    height: 100%;
    width: 100%;
    overflow: hidden;
}
</style>