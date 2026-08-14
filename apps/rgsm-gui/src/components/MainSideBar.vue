<script lang="ts" setup>
import { computed, ref } from 'vue';
import FavoriteSideBar from './FavoriteSideBar.vue';
import { Files, Search, Star, Menu } from '@element-plus/icons-vue';
import { $t } from '../i18n';
import { debug } from '../utils/logger';
import { useNavigationLinks } from '../composables/useNavigationLinks';
import { getGameManagementPath } from '../composables/useGameManagementRoute';
import { useSidebarResize } from '../composables/useSidebarResize';
import { useSaveListExpandBehavior } from '../composables/useSaveListExpandBehavior';
import type { MenuInstance } from '../ui/elementPlus/menu';

const { config, isGameVisible } = useConfig();
const { baseLinks } = useNavigationLinks();
const { sortedGames } = useSaveListSort();
const { isResizing, startResize } = useSidebarResize({
  minWidth: 200,
  maxWidth: 400,
});

const games = computed(() => {
  return sortedGames(
    config.value.games.filter((game) => isGameVisible(game.storage_key, game.name))
  );
});

const router = useRouter();
const route = useRoute();
const show_favorite = ref(false);
const searchQuery = ref('');

const menuRef = ref<MenuInstance>();
const saveListMenuIndex = 'save-list';

// 过滤菜单项
const filteredGames = computed(() => {
  const query = searchQuery.value.trim().toLowerCase();
  if (!query) return games.value;
  return games.value.filter((game) => game.name.toLowerCase().includes(query));
});

// 过滤常规菜单
const filteredLinks = computed(() => {
  const query = searchQuery.value.trim().toLowerCase();
  if (!query) return baseLinks.value;
  return baseLinks.value.filter((link) => link.text.toLowerCase().includes(query));
});

const { saveListDefaultOpeneds, handleMenuOpen, handleMenuClose } = useSaveListExpandBehavior({
  menuRef,
  filteredGames,
  showFavorite: show_favorite,
  searchQuery,
});

function select_handler(key: string, keyPath: string) {
  const targetPath = keyPath[keyPath.length - 1];
  debug(`${$t('misc.navigate_to')} ${targetPath}`);
  if (targetPath) {
    router.push(targetPath);
  }
}

// 清除搜索
function clearSearch() {
  searchQuery.value = '';
}
</script>

<template>
  <div class="sidebar-wrapper">
    <ElContainer class="main-side-bar">
      <!-- 顶部搜索和切换区域 -->
      <div class="sidebar-header">
        <div class="view-toggle">
          <el-tooltip
            :content="show_favorite ? $t('misc.menu') : $t('misc.favorites')"
            placement="top"
          >
            <el-button
              circle
              size="small"
              :type="show_favorite ? 'primary' : 'default'"
              @click="show_favorite = !show_favorite"
            >
              <el-icon>
                <component :is="show_favorite ? Star : Menu" />
              </el-icon>
            </el-button>
          </el-tooltip>
        </div>
        <div class="search-container">
          <el-input
            v-model="searchQuery"
            :placeholder="$t('misc.search')"
            clearable
            size="small"
            @clear="clearSearch"
          >
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
          <el-menu
            v-if="!show_favorite"
            ref="menuRef"
            class="menu-item"
            :default-active="route.path"
            :select="select_handler"
            :router="true"
            :collapse-transition="false"
            :default-openeds="saveListDefaultOpeneds"
            @open="handleMenuOpen"
            @close="handleMenuClose"
          >
            <!-- 存档栏 -->
            <el-sub-menu
              v-if="filteredGames.length > 0 || !searchQuery.trim()"
              :index="saveListMenuIndex"
            >
              <template #title>
                <el-icon>
                  <Files />
                </el-icon>
                <span>{{ $t('misc.save_manage') }}</span>
              </template>
              <el-menu-item
                v-for="game in filteredGames"
                :key="game.name"
                :index="getGameManagementPath(game.name)"
              >
                {{ game.name }}
              </el-menu-item>
            </el-sub-menu>
            <!-- 常规按钮 -->
            <el-menu-item v-for="link in filteredLinks" :key="link.link" :index="link.link">
              <el-icon>
                <component :is="link.icon" />
              </el-icon>
              <span>{{ link.text }}</span>
            </el-menu-item>
          </el-menu>
          <FavoriteSideBar v-else :search-query="searchQuery" />
        </ElRow>
      </ElScrollbar>
    </ElContainer>
    <!-- 拖动调整大小的区域 -->
    <div class="resize-handle" :class="{ active: isResizing }" @mousedown="startResize" />
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
