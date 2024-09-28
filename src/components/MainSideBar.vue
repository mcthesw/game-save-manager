<script lang="ts" setup>
import { computed, ref } from "vue";
import FavoriteSideBar from "./FavoriteSideBar.vue";
import {
    DocumentAdd,
    Files,
    InfoFilled,
    HotWater,
    Setting,
    MostlyCloudy
} from "@element-plus/icons-vue";
import { useRoute, useRouter } from "vue-router";
import { useConfig } from "../stores/ConfigFile";
import { $t } from "../i18n";
import { ElContainer, ElIcon, ElRow, ElScrollbar } from "element-plus";

let config = useConfig();

// TODO:抽离到新文件中，同时`Settings.vue`也要抽离
const links = computed(() => [
    { text: $t("sidebar.homepage"), link: "/home", icon: HotWater },
    { text: $t("sidebar.add_game"), link: "/add-game", icon: DocumentAdd },
    { text: $t("sidebar.sync_settings"), link: "/sync-settings", icon: MostlyCloudy },
    { text: $t("sidebar.settings"), link: "/settings", icon: Setting },
    { text: $t("sidebar.about"), link: "/about", icon: InfoFilled },
]);

const games = computed(() => {
    return config.games;
});

const router = useRouter()
const route = useRoute()
const show_favorite = ref(false)
function select_handler(key: string, keyPath: string) {
    console.log($t('misc.navigate_to'), keyPath[keyPath.length - 1]);
    router.push(keyPath[keyPath.length - 1]);
}
</script>

<template>
    <ElContainer class="main-side-bar">
        <ElRow>
            <el-switch class="favorite-switch" v-model="show_favorite" inline-prompt :active-text="$t('misc.favorites')"
                :inactive-text="$t('misc.menu')" />
        </ElRow>
        <ElScrollbar>
            <ElRow class="main-menu-container">
                <el-menu class="menu-item" :default-active="route.path" :select="select_handler" :router="true"
                    v-if="!show_favorite">
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
                </el-menu>
                <FavoriteSideBar v-else />
            </ElRow>
        </ElScrollbar>
    </ElContainer>
</template>

<style scoped>
.main-side-bar {
    height: 100%;
    flex-direction: column;
    border-right: 1px solid var(--el-border-color);
    overflow: hidden;
}

/**
由于el-menu-item的默认样式会导致文字溢出，所以需要手动设置
然而即使这样也只支持两行，超过两行的文字会很难看
*/
.el-menu-item {
    white-space: normal !important;
    line-height: normal !important;
}

.el-menu {
    border: none;
}

.favorite-switch {
    margin: auto;
}

.menu-item {
    width: 100%;
}

.main-menu-container {
    flex-grow: 1;
}
</style>