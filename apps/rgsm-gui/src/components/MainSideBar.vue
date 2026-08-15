<script lang="ts" setup>
import { computed, onMounted, ref, watch } from 'vue';
import { Home, Info, Plus, Settings, Star } from '@lucide/vue';
import { v4 as uuidv4 } from 'uuid';
import { $t } from '../i18n';
import { error } from '../utils/logger';
import { commands, type FavoriteTreeNode, type Game } from '../api/commands';
import { getGameManagementPath } from '../composables/useGameManagementRoute';
import { useSidebarResize } from '../composables/useSidebarResize';
import KButton from '../ui/kit/KButton.vue';
import KInput from '../ui/kit/KInput.vue';
import FavoriteTree from './FavoriteTree.vue';
import { collectLeafNames } from './favoriteTreeContext';

const { config, isGameVisible, saveConfig } = useConfig();
const { sortedGames } = useSaveListSort();
const { isResizing, startResize } = useSidebarResize({
  minWidth: 200,
  maxWidth: 400,
});

const router = useRouter();
const route = useRoute();
const searchQuery = ref('');

// ——— 导航（主页/设置/关于）———
const navLinks = computed(() => [
  { text: $t('sidebar.homepage'), link: '/', icon: Home },
  { text: $t('sidebar.settings'), link: '/Settings', icon: Settings },
  { text: $t('sidebar.about'), link: '/About', icon: Info },
]);

// ——— 游戏列表（「全部」视图） ———
const games = computed(() =>
  sortedGames(config.value.games.filter((game) => isGameVisible(game.storage_key, game.name)))
);

const visibleGames = computed(() => {
  const query = searchQuery.value.trim().toLowerCase();
  if (!query) return games.value;
  return games.value.filter((game) => game.name.toLowerCase().includes(query));
});

// 收藏叶子集合：「全部」视图的星标状态；树本身的组织在 FavoriteTree 内
const favoriteNames = computed(() => collectLeafNames(config.value?.favorites));

function removeFavoriteLeaf(nodes: FavoriteTreeNode[], name: string): boolean {
  const index = nodes.findIndex((node) => node.is_leaf && node.label === name);
  if (index >= 0) {
    nodes.splice(index, 1);
    return true;
  }
  for (const node of nodes) {
    if (!node.is_leaf && node.children && removeFavoriteLeaf(node.children, name)) return true;
  }
  return false;
}

async function toggleFavorite(game: Game) {
  if (!config.value) return;
  const favorites = [...(config.value.favorites ?? [])];
  if (favoriteNames.value.has(game.name)) {
    removeFavoriteLeaf(favorites, game.name);
  } else {
    favorites.push({
      label: game.name,
      is_leaf: true,
      children: null,
      node_id: uuidv4(),
    });
  }
  config.value.favorites = favorites;
  await saveConfig();
}

// ——— 视图切换：收藏夹 / 全部。无收藏的用户默认落在「全部」 ———
// config 初值是同步 DEFAULT_CONFIG（空），必须等真实配置到达再判断
const viewMode = ref<'favorites' | 'all'>('favorites');
let viewInitialized = false;
watch(
  () => config.value,
  (cfg) => {
    if (viewInitialized || !cfg) return;
    if (cfg.games.length === 0 && collectLeafNames(cfg.favorites).size === 0) return;
    viewInitialized = true;
    if (collectLeafNames(cfg.favorites).size === 0) viewMode.value = 'all';
  },
  { immediate: true }
);

// ——— 状态：自动备份圆点 ———
const autoBackupGames = ref<Set<string>>(new Set());

async function refreshAutoBackup() {
  try {
    const result = await commands.getAutoBackupStatus();
    if (result.status === 'ok') {
      autoBackupGames.value = new Set(result.data.map((row) => row.game_name));
    }
  } catch (e) {
    error(`refresh auto-backup status error: ${e}`);
  }
}

onMounted(refreshAutoBackup);
watch(() => config.value?.games?.length, refreshAutoBackup);

function isActive(path: string): boolean {
  return route.path === path;
}

function goGame(game: Game) {
  router.push(getGameManagementPath(game.name));
}
</script>

<template>
  <div class="sidebar-wrapper">
    <aside class="sidebar">
      <div class="sidebar-search">
        <KInput v-model="searchQuery" size="sm" :placeholder="$t('misc.search')" />
      </div>

      <nav class="sidebar-nav">
        <button
          v-for="link in navLinks"
          :key="link.link"
          type="button"
          class="side-row nav-row"
          :class="{ active: isActive(link.link) }"
          @click="router.push(link.link)"
        >
          <component :is="link.icon" :size="15" class="row-icon" />
          <span class="row-text">{{ link.text }}</span>
        </button>
      </nav>

      <div class="games-head">
        <span class="games-title">{{ $t('sidebar.games') }}</span>
        <KButton size="sm" variant="primary" @click="router.push('/AddGame')">
          <template #icon><Plus :size="13" /></template>
          {{ $t('sidebar.add_game') }}
        </KButton>
      </div>

      <div class="seg" role="tablist" :aria-label="$t('sidebar.games')">
        <button
          type="button"
          role="tab"
          class="seg-btn"
          :class="{ active: viewMode === 'favorites' }"
          :aria-selected="viewMode === 'favorites'"
          @click="viewMode = 'favorites'"
        >
          {{ $t('misc.favorites') }}
        </button>
        <button
          type="button"
          role="tab"
          class="seg-btn"
          :class="{ active: viewMode === 'all' }"
          :aria-selected="viewMode === 'all'"
          @click="viewMode = 'all'"
        >
          {{ $t('sidebar.all_games') }}
        </button>
      </div>

      <div class="games-scroll">
        <FavoriteTree v-show="viewMode === 'favorites'" :search-query="searchQuery" />

        <div v-show="viewMode === 'all'" class="all-list">
          <button
            v-for="game in visibleGames"
            :key="game.name"
            type="button"
            class="side-row game-row"
            :class="{ active: isActive(getGameManagementPath(game.name)) }"
            :title="game.name"
            @click="goGame(game)"
          >
            <span
              class="game-dot"
              :class="{ on: autoBackupGames.has(game.name) }"
              :title="autoBackupGames.has(game.name) ? $t('sidebar.auto_backup_on') : undefined"
            />
            <span class="row-text">{{ game.name }}</span>
            <span
              class="game-star"
              :class="{ faved: favoriteNames.has(game.name) }"
              role="button"
              :aria-label="
                favoriteNames.has(game.name)
                  ? $t('favorite.remove')
                  : $t('favorite.add_to_favorite')
              "
              @click.stop="toggleFavorite(game)"
            >
              <Star :size="13" :fill="favoriteNames.has(game.name) ? 'currentColor' : 'none'" />
            </span>
          </button>
          <p v-if="visibleGames.length === 0 && searchQuery.trim()" class="empty-hint">
            {{ $t('misc.no_search_results') }}
          </p>
        </div>
      </div>
    </aside>
    <!-- 拖动调整大小的区域 -->
    <div class="resize-handle" :class="{ active: isResizing }" @mousedown="startResize" />
  </div>
</template>

<style scoped>
.sidebar-wrapper {
  position: relative;
  display: flex;
  height: 100%;
}

.sidebar {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  overflow: hidden;
  background: var(--bg);
  border-right: 1px solid var(--border);
  font-family: var(--font-sans-stack);
}

.sidebar-search {
  padding: 10px 12px;
  border-bottom: 1px solid var(--border);
}

.sidebar-nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px 8px 4px;
}

.side-row {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 8px;
  border: none;
  border-radius: var(--radius-sm);
  background: none;
  font: inherit;
  font-size: 0.85rem;
  color: var(--text-dim);
  cursor: pointer;
  text-align: left;
  transition:
    background-color 0.15s ease,
    color 0.15s ease;
}

.side-row:hover {
  background: var(--surface-2);
  color: var(--text);
}

.side-row.active {
  background: var(--surface-2);
  color: var(--text);
  font-weight: 600;
}

.side-row:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}

.row-icon {
  flex-shrink: 0;
}

.row-text {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.games-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 10px 12px 6px;
}

.games-title {
  font-size: 0.75rem;
  font-weight: 600;
  letter-spacing: 0.04em;
  color: var(--text-dim);
}

.seg {
  display: flex;
  gap: 2px;
  margin: 0 12px 8px;
  padding: 2px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
}

.seg-btn {
  flex: 1;
  padding: 3px 0;
  border: none;
  border-radius: calc(var(--radius-sm) - 2px);
  background: transparent;
  font: inherit;
  font-size: 0.78rem;
  color: var(--text-dim);
  cursor: pointer;
  transition:
    background-color 0.15s ease,
    color 0.15s ease;
}

.seg-btn:hover {
  color: var(--text);
}

.seg-btn.active {
  background: var(--surface-2);
  color: var(--text);
  font-weight: 600;
}

.seg-btn:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}

.games-scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 0 8px 12px;
}

.all-list {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.game-dot {
  flex-shrink: 0;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  border: 1px solid var(--border-strong);
}

.game-dot.on {
  border-color: var(--success);
  background: var(--success);
}

.game-star {
  display: inline-flex;
  align-items: center;
  flex-shrink: 0;
  padding: 2px;
  border-radius: var(--radius-sm);
  color: var(--text-dim);
  opacity: 0;
  transition:
    opacity 0.15s ease,
    color 0.15s ease;
}

.game-row:hover .game-star,
.game-star.faved {
  opacity: 1;
}

.game-star.faved {
  color: var(--text);
}

.game-star:hover {
  color: var(--text);
  background: var(--surface);
}

.empty-hint {
  margin: 12px 8px;
  font-size: 0.8rem;
  color: var(--text-dim);
}

/* 拖动调整大小的区域 */
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
  background-color: var(--border-strong);
}
</style>
