<script lang="ts" setup>
import { computed } from "vue";
import {
    DocumentAdd,
    Files,
    InfoFilled,
    HotWater,
    Setting,
} from "@element-plus/icons-vue";
import { useRouter } from "vue-router";

const links = [
    { text: "欢迎界面", link: "/home", icon: HotWater },
    { text: "添加游戏", link: "/add-game", icon: DocumentAdd },
    { text: "设置", link: "/settings", icon: Setting },
    { text: "关于", link: "/about", icon: InfoFilled },
];

const games = computed(() => {
    // TODO: 从pinia中获取游戏列表
    return ["123", "456"];
});

const router = useRouter()
function select_handler(key: string, keyPath: string) {
    console.log("导航至", keyPath[keyPath.length - 1]);
    router.push(keyPath[keyPath.length - 1]);
}

</script>

<template>
    <el-menu class="main-side-bar" default-active="/home" :select="select_handler" :router="true">
        <el-scrollbar>
            <!-- 下方是存档栏 -->
            <el-sub-menu index="1">
                <template #title>
                    <el-icon>
                        <Files></Files>
                    </el-icon>
                    <span>存档管理</span>
                </template>
                <el-menu-item v-for="game in Object.keys(games)" :key="game" :index="'/management/' + game">
                    {{ game }}
                </el-menu-item>
            </el-sub-menu>
            <!-- 下方是常规按钮 -->
            <el-menu-item v-for="link in links" :index="link.link" :key="link.link">
                <el-icon>
                    <component :is="link.icon"></component>
                </el-icon>
                <span>{{ link.text }}</span>
            </el-menu-item>
        </el-scrollbar>
    </el-menu>
</template>

<style scoped>
.main-side-bar {
    height: 100%;
}
</style>