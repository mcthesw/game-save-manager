<script lang="ts" setup>
import { computed, ref, watch, inject, onMounted, nextTick } from "vue";
import FavoriteSideBar from "./FavoriteSideBar.vue";
import {
    DocumentAdd,
    Files,
    InfoFilled,
    HotWater,
    Setting,
    MostlyCloudy,
    Search,
    Star,
    Menu
} from "@element-plus/icons-vue";
import { $t } from "../i18n";
import { debug } from "@tauri-apps/plugin-log";
import type { MenuInstance } from "element-plus";

let { config, saveConfig } = useConfig();

const min_width = ref(200);
const max_width = ref(400);

// TODO:抽离到新文件中，同时`Settings.vue`也要抽离
const links = computed(() => [
    { text: $t("sidebar.homepage"), link: "/", icon: HotWater },
    { text: $t("sidebar.add_game"), link: "/AddGame", icon: DocumentAdd },
    { text: $t("sidebar.sync_settings"), link: "/SyncSettings", icon: MostlyCloudy },
    { text: $t("sidebar.settings"), link: "/Settings", icon: Setting },
    { text: $t("sidebar.about"), link: "/About", icon: InfoFilled },
]);

const games = computed(() => {
    return config.value.games;
});

const router = useRouter()
const route = useRoute()
const show_favorite = ref(false)
const searchQuery = ref('')

const menuRef = ref<MenuInstance>()
const saveListMenuIndex = 'save-list'

function getSaveListBehavior() {
    return config.value.settings?.save_list_expand_behavior ?? 'always_closed'
}

function getSavedExpandState() {
    return config.value.settings?.save_list_last_expanded ?? false
}

function shouldExpandSaveList() {
    const behavior = getSaveListBehavior()
    if (behavior === 'always_open') {
        return true
    }
    if (behavior === 'remember_last') {
        return getSavedExpandState()
    }
    return false
}

const saveListDefaultOpeneds = computed(() => shouldExpandSaveList() ? [saveListMenuIndex] : [])

// 从父组件注入侧边栏宽度
const sidebarWidth = inject('sidebarWidth', ref(240))
const isResizing = ref(false)
const startX = ref(0)
const startWidth = ref(0)

// 过滤菜单项
const filteredGames = computed(() => {
    if (!searchQuery.value) return games.value;
    const query = searchQuery.value.toLowerCase();
    return games.value.filter(game => game.name.toLowerCase().includes(query));
});

// 过滤常规菜单
const filteredLinks = computed(() => {
    if (!searchQuery.value) return links.value;
    const query = searchQuery.value.toLowerCase();
    return links.value.filter(link => link.text.toLowerCase().includes(query));
});
function select_handler(key: string, keyPath: string) {
    debug(`${$t('misc.navigate_to')} ${keyPath[keyPath.length - 1]}`);
    router.push(keyPath[keyPath.length - 1]);
}

// 侧边栏大小调整处理函数
function startResize(event: MouseEvent) {
    event.preventDefault();
    isResizing.value = true;
    startX.value = event.clientX;
    startWidth.value = sidebarWidth.value;
    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', stopResize);
}

function handleMouseMove(event: MouseEvent) {
    if (!isResizing.value) return;
    const delta = event.clientX - startX.value;
    // 设置最小和最大宽度限制
    const newWidth = Math.max(min_width.value, Math.min(max_width.value, startWidth.value + delta));
    sidebarWidth.value = newWidth;
}

function stopResize() {
    isResizing.value = false;
    document.removeEventListener('mousemove', handleMouseMove);
    document.removeEventListener('mouseup', stopResize);
}

// 清除搜索
function clearSearch() {
    searchQuery.value = '';
}

async function applySaveListExpandState() {
    await nextTick();
    const menu = menuRef.value;
    if (!menu) {
        return;
    }
    if (shouldExpandSaveList()) {
        menu.open(saveListMenuIndex);
    } else {
        menu.close(saveListMenuIndex);
    }
}

async function persistSaveListState(expanded: boolean) {
    if (getSavedExpandState() === expanded) {
        return;
    }
    if (!config.value.settings) {
        return;
    }
    config.value.settings.save_list_last_expanded = expanded;
    await saveConfig();
}

async function handleMenuOpen(index: string) {
    if (index !== saveListMenuIndex) {
        return;
    }
    if (getSaveListBehavior() === 'remember_last') {
        await persistSaveListState(true);
    }
}

async function handleMenuClose(index: string) {
    if (index !== saveListMenuIndex) {
        return;
    }
    if (getSaveListBehavior() === 'remember_last') {
        await persistSaveListState(false);
    }
}

watch(
    () => config.value.settings?.save_list_expand_behavior,
    async (behavior) => {
        await applySaveListExpandState();
        if (behavior === 'always_open') {
            await persistSaveListState(true);
        } else if (behavior === 'always_closed') {
            await persistSaveListState(false);
        }
    },
    { immediate: true }
);

watch(
    () => config.value.settings?.save_list_last_expanded,
    async () => {
        if (getSaveListBehavior() === 'remember_last') {
            await applySaveListExpandState();
        }
    }
);

watch(
    filteredGames,
    () => {
        void applySaveListExpandState();
    },
    { deep: true }
);

watch(show_favorite, (value) => {
    if (!value) {
        void applySaveListExpandState();
    }
});

onMounted(() => {
    void applySaveListExpandState();
});
</script>

<template>
    <div class="sidebar-wrapper">
        <ElContainer class="main-side-bar">
            <!-- 顶部搜索和切换区域 -->
            <div class="sidebar-header">
                <div class="view-toggle">
                    <el-tooltip :content="show_favorite ? $t('misc.menu') : $t('misc.favorites')" placement="top">
                        <el-button circle size="small" @click="show_favorite = !show_favorite"
                            :type="show_favorite ? 'primary' : 'default'">
                            <el-icon>
                                <component :is="show_favorite ? Star : Menu"></component>
                            </el-icon>
                        </el-button>
                    </el-tooltip>
                </div>
                <div class="search-container">
                    <el-input v-model="searchQuery" :placeholder="$t('misc.search')" clearable @clear="clearSearch"
                        size="small">
                        <template #prefix>
                            <el-icon>
                                <Search />
                            </el-icon>
                        </template>
                    </el-input>
                </div>

            </div>

            <!-- 内容区域 -->
            <ElScrollbar always>
                <ElRow class="main-menu-container">
                    <el-menu ref="menuRef" class="menu-item" :default-active="route.path" :select="select_handler"
                        :router="true" v-if="!show_favorite" :collapse-transition="false"
                        :default-openeds="saveListDefaultOpeneds" @open="handleMenuOpen" @close="handleMenuClose">
                        <!-- 存档栏 -->
                        <el-sub-menu :index="saveListMenuIndex" v-if="filteredGames.length > 0 || !searchQuery">
                            <template #title>
                                <el-icon>
                                    <Files></Files>
                                </el-icon>
                                <span>{{ $t('misc.save_manage') }}</span>
                            </template>
                            <el-menu-item v-for="game in filteredGames" :key="game.name"
                                :index="'/Management/' + game.name">
                                {{ game.name }}
                            </el-menu-item>
                        </el-sub-menu>
                        <!-- 常规按钮 -->
                        <el-menu-item v-for="link in filteredLinks" :index="link.link" :key="link.link">
                            <el-icon>
                                <component :is="link.icon"></component>
                            </el-icon>
                            <span>{{ link.text }}</span>
                        </el-menu-item>
                    </el-menu>
                    <FavoriteSideBar v-else :searchQuery="searchQuery" />
                </ElRow>
            </ElScrollbar>

        </ElContainer>
        <!-- 拖动调整大小的区域 -->
        <div class="resize-handle" @mousedown="startResize" :class="{ 'active': isResizing }"></div>
    </div>
</template>

<style scoped>
.sidebar-wrapper {
    position: relative;
    height: 100%;
    display: flex;
}

.main-side-bar {
    height: 100%;
    flex-direction: column;
    border-right: 1px solid var(--el-border-color);
    overflow: hidden;
    transition: width 0.2s ease;
    background-color: var(--el-bg-color);
    box-shadow: 0 2px 12px 0 rgba(0, 0, 0, 0.1);
    /* 禁止横向滚动 */
    overflow-x: hidden;
}

/**
由于el-menu-item的默认样式会导致文字溢出，所以需要手动设置
*/
.el-menu-item {
    white-space: normal !important;
    line-height: normal !important;
    padding: 12px 20px !important;
    height: auto !important;
    min-height: 50px;
    display: flex;
    align-items: center;
    /* 确保文本换行且不会导致横向滚动 */
    word-break: break-word;
    overflow-wrap: break-word;
    max-width: 100%;
}

.el-menu {
    border: none;
}

.menu-item {
    width: 100%;
}

.main-menu-container {
    flex-direction: column;
    flex-grow: 1;
    padding: 0 8px;
}

/* 顶部搜索和切换区域样式 */
.sidebar-header {
    display: flex;
    align-items: center;
    padding: 12px 16px;
    border-bottom: 1px solid var(--el-border-color-lighter);
    gap: 10px;
    background-color: var(--el-bg-color-overlay);
}

.search-container {
    flex-grow: 1;
}

.view-toggle {
    display: flex;
    align-items: center;
}

/* 拖动调整大小的区域样式 */
.resize-handle {
    position: absolute;
    top: 0;
    right: -5px;
    width: 10px;
    height: 100%;
    cursor: col-resize;
    background-color: transparent;
    transition: background-color 0.2s;
    z-index: 100;
}

.resize-handle:hover,
.resize-handle.active {
    background-color: var(--el-color-primary);
}

/* 优化子菜单样式 */
:deep(.el-sub-menu__title) {
    height: auto !important;
    min-height: 50px;
    line-height: normal !important;
    padding: 12px 20px !important;
    /* 确保文本换行且不会导致横向滚动 */
    word-break: break-word;
    overflow-wrap: break-word;
    max-width: 100%;
    /* 增加主菜单的视觉区分度 */
    font-weight: 600;
    background-color: var(--el-bg-color-overlay);
    border-radius: 6px;
}

/* 优化菜单项图标与文字间距 */
:deep(.el-menu-item .el-icon),
:deep(.el-sub-menu__title .el-icon) {
    margin-right: 10px;
    flex-shrink: 0;
}

/* 增加子菜单项的视觉区分度 */
:deep(.el-menu-item) {
    margin: 4px 0;
    border-radius: 6px;
}

:deep(.el-menu-item:hover) {
    background-color: var(--el-fill-color-light);
}

:deep(.el-menu-item.is-active) {
    background-color: var(--el-color-primary-light-9);
    color: var(--el-color-primary);
    border-left: 3px solid var(--el-color-primary);
}

/* 优化菜单项文字溢出处理 - 允许完整显示长文本 */
:deep(.el-menu-item span),
:deep(.el-sub-menu__title span) {
    overflow: visible;
    white-space: normal;
    word-break: break-word;
    line-height: 1.4;
    /* 确保文本不会导致横向滚动 */
    max-width: 100%;
    display: inline-block;
}
</style>