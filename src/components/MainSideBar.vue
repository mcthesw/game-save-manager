<script lang="ts" setup>
import { computed } from "vue";
import {
    DocumentAdd,
    Files,
    InfoFilled,
    HotWater,
    Setting,
    MostlyCloudy,
} from "@element-plus/icons-vue";
import { useRoute, useRouter } from "vue-router";
import { useConfig } from "../stores/ConfigFile";
import { $t } from "../i18n";

let config = useConfig();

const links = [
    { text: $t("sidebar.homepage"), link: "/home", icon: HotWater },
    { text: $t("sidebar.add_game"), link: "/add-game", icon: DocumentAdd },
    { text: $t("sidebar.sync_settings"), link: "/sync-settings", icon: MostlyCloudy },
    { text: $t("sidebar.settings"), link: "/settings", icon: Setting },
    { text: $t("sidebar.about"), link: "/about", icon: InfoFilled },
];

const games = computed(() => {
    return config.games;
});

const router = useRouter()
const route = useRoute()
function select_handler(key: string, keyPath: string) {
    console.log($t('misc.navigate_to'), keyPath[keyPath.length - 1]);
    router.push(keyPath[keyPath.length - 1]);
}

</script>

<template>
    <el-menu class="main-side-bar" :default-active="route.path" :select="select_handler" :router="true">
        <el-scrollbar>
            <!-- 下方是存档栏 -->
            <el-sub-menu index="1">
                <template #title>
                    <el-icon>
                        <Files></Files>
                    </el-icon>
                    <span>{{ $t('misc.save_manage') }}</span>
                </template>
                <el-menu-item v-for="game in games" :key="game.name" :index="'/management/' + game.name">
                    {{ game.name }}
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

/**
由于el-menu-item的默认样式会导致文字溢出，所以需要手动设置
然而即使这样也只支持两行，超过两行的文字会很难看
*/
.el-menu-item {
    white-space: normal !important;
    line-height: normal !important;
}
</style>