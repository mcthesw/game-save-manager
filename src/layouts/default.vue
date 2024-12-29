<script setup lang="ts">
</script>

<template>
    <ElContainer>
        <ElAside width="200px">
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