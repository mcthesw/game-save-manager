<script lang="ts" setup>
import { computed, provide, ref, watch } from 'vue';
import { useDebounceFn } from '@vueuse/core';
import { FolderPlus, Pencil, Plus } from '@lucide/vue';
import { v4 as uuidv4 } from 'uuid';
import { $t } from '../i18n';
import { error } from '../utils/logger';
import { type FavoriteTreeNode as FavNode, type Game } from '../api/commands';
import { getGameManagementPath } from '../composables/useGameManagementRoute';
import { notifySuccess, notifyWarning } from '../composables/useActivityCenter';
import KButton from '../ui/kit/KButton.vue';
import KDialog from '../ui/kit/KDialog.vue';
import KTooltip from '../ui/kit/KTooltip.vue';
import FavoriteTreeNode from './FavoriteTreeNode.vue';
import {
  FAVORITE_TREE_CTX,
  collectFolderIds,
  collectLeafNames,
  countLeaves,
  filterTree,
  findNode,
  isInsideSubtree,
  moveNode,
  removeById,
  type DropPos,
} from './favoriteTreeContext';

const props = withDefaults(defineProps<{ searchQuery?: string }>(), { searchQuery: '' });

const { config, saveConfig } = useConfig();
const feedback = useFeedback();
const router = useRouter();

const editMode = ref(false);
const expandedIds = ref<Set<string>>(new Set());
const dragId = ref<string | null>(null);
const dropTarget = ref<{ id: string; pos: DropPos } | null>(null);
const addDialogOpen = ref(false);

const searching = computed(() => !!props.searchQuery.trim());
const rootNodes = computed(() => config.value?.favorites ?? []);
const visibleTree = computed(() => filterTree(rootNodes.value, props.searchQuery));
const favoriteNames = computed(() => collectLeafNames(config.value?.favorites));

// ——— 持久化 ———
const persist = useDebounceFn(async () => {
  try {
    await saveConfig();
  } catch (e) {
    error(`save favorites error: ${e}`);
  }
}, 500);

function commitTree() {
  if (!config.value) return;
  // 根数组换新引用以触发渲染；子数组已在原地修改
  config.value.favorites = [...rootNodes.value];
  persist();
}

// ——— 展开：跟随「默认展开收藏树」设置 ———
// config 初值是同步 DEFAULT_CONFIG（空），必须等真实配置到达再应用
let expandInitialized = false;
watch(
  () => config.value,
  (cfg) => {
    if (expandInitialized || !cfg) return;
    if (cfg.games.length === 0 && (cfg.favorites?.length ?? 0) === 0) return;
    expandInitialized = true;
    if (cfg.settings.default_expend_favorites_tree) {
      expandedIds.value = collectFolderIds(cfg.favorites);
    }
  },
  { immediate: true }
);

function toggleExpand(id: string) {
  const next = new Set(expandedIds.value);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  expandedIds.value = next;
}

// ——— 叶子点击：跳转游戏页；收藏指向已删除游戏时提示 ———
function clickLeaf(node: FavNode) {
  if (!config.value?.games.find((game) => game.name === node.label)) {
    notifyWarning($t('favorite.game_not_found') + ': ' + node.label);
    return;
  }
  router.push(getGameManagementPath(node.label));
}

// ——— 编辑操作 ———
// 删除含收藏游戏的文件夹需确认（会一并移出收藏）；空文件夹与单个游戏静默直删
async function removeNode(id: string) {
  if (!config.value) return;
  const node = findNode(rootNodes.value, id);
  if (!node) return;
  if (!node.is_leaf) {
    const leaves = countLeaves(node);
    if (leaves > 0) {
      try {
        await feedback.confirm(
          $t('favorite.confirm_remove_folder', { count: leaves }),
          $t('home.hint'),
          {
            type: 'warning',
            confirmButtonText: $t('settings.confirm'),
            cancelButtonText: $t('settings.cancel'),
          }
        );
      } catch {
        return;
      }
    }
  }
  if (!removeById(rootNodes.value, id)) return;
  commitTree();
  notifySuccess($t('favorite.remove_success'));
}

async function addFolder() {
  let result;
  try {
    result = await feedback.prompt($t('favorite.new_folder_name'), $t('home.hint'), {
      confirmButtonText: $t('settings.confirm'),
      cancelButtonText: $t('settings.cancel'),
    });
  } catch {
    return;
  }
  const name = result.value.trim();
  if (!name || rootNodes.value.some((node) => node.label === name)) {
    notifyWarning($t('favorite.duplicated_empty_error'));
    return;
  }
  rootNodes.value.push({ label: name, is_leaf: false, children: [], node_id: uuidv4() });
  commitTree();
}

function addGame(game: Game) {
  if (!config.value || favoriteNames.value.has(game.name)) return;
  rootNodes.value.push({ label: game.name, is_leaf: true, children: null, node_id: uuidv4() });
  commitTree();
  notifySuccess($t('favorite.add_success') + ': ' + game.name);
}

function gamePathsText(game: Game): string {
  return Object.values(game.game_paths ?? {}).join(' ; ');
}

async function addAllGames() {
  try {
    await feedback.confirm($t('favorite.confirm_add_all_games'), $t('home.hint'), {
      type: 'warning',
      confirmButtonText: $t('settings.confirm'),
      cancelButtonText: $t('settings.cancel'),
    });
  } catch {
    return;
  }
  if (!config.value) return;
  const existing = favoriteNames.value;
  const fresh = config.value.games
    .filter((game) => !existing.has(game.name))
    .map((game) => ({
      label: game.name,
      is_leaf: true,
      children: null,
      node_id: uuidv4(),
    }));
  if (fresh.length === 0) {
    notifyWarning($t('favorite.no_new_games'));
    return;
  }
  config.value.favorites = [...rootNodes.value, ...fresh];
  persist();
  notifySuccess($t('favorite.add_all_success', { count: fresh.length }));
}

// ——— 拖拽（仅编辑模式可拖，由节点上的 draggable 属性控制） ———
function onDragStart(id: string, e: DragEvent) {
  dragId.value = id;
  if (e.dataTransfer) {
    e.dataTransfer.effectAllowed = 'move';
    e.dataTransfer.setData('text/plain', id);
  }
}

function onDragOver(id: string, isLeaf: boolean, e: DragEvent) {
  const dragging = dragId.value;
  if (!dragging || dragging === id) return;
  const dragNode = findNode(rootNodes.value, dragging);
  if (!dragNode || isInsideSubtree(dragNode, id)) return;

  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
  const y = e.clientY - rect.top;
  let pos: DropPos;
  if (isLeaf) {
    pos = y < rect.height / 2 ? 'before' : 'after';
  } else if (y < rect.height * 0.25) {
    pos = 'before';
  } else if (y > rect.height * 0.75) {
    pos = 'after';
  } else {
    pos = 'inner';
  }
  if (pos === 'inner' && isLeaf) return;

  e.preventDefault();
  if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
  dropTarget.value = { id, pos };
}

function onDragLeave(id: string) {
  if (dropTarget.value?.id === id) dropTarget.value = null;
}

function clearDrag() {
  dragId.value = null;
  dropTarget.value = null;
}

function onDrop(id: string) {
  const target = dropTarget.value;
  if (dragId.value && target && target.id === id) {
    if (moveNode(rootNodes.value, dragId.value, id, target.pos)) commitTree();
  }
  clearDrag();
}

provide(FAVORITE_TREE_CTX, {
  editMode,
  searching,
  expandedIds,
  toggleExpand,
  dropTarget,
  onDragStart,
  onDragOver,
  onDragLeave,
  onDrop,
  onDragEnd: clearDrag,
  removeNode,
  clickLeaf,
});
</script>

<template>
  <div class="fav-tree">
    <div class="fav-actions">
      <KTooltip :content="$t('favorite.add_favorite_folder')" side="bottom">
        <KButton
          size="sm"
          variant="ghost"
          :aria-label="$t('favorite.add_favorite_folder')"
          @click="addFolder"
        >
          <template #icon><FolderPlus :size="14" /></template>
        </KButton>
      </KTooltip>
      <KTooltip :content="$t('favorite.add_game')" side="bottom">
        <KButton
          size="sm"
          variant="ghost"
          :aria-label="$t('favorite.add_game')"
          @click="addDialogOpen = true"
        >
          <template #icon><Plus :size="14" /></template>
        </KButton>
      </KTooltip>
      <KTooltip :content="$t('favorite.enable_edit')" side="bottom">
        <KButton
          size="sm"
          :variant="editMode ? 'default' : 'ghost'"
          :aria-label="$t('favorite.enable_edit')"
          :aria-pressed="editMode"
          @click="editMode = !editMode"
        >
          <template #icon><Pencil :size="14" /></template>
        </KButton>
      </KTooltip>
    </div>

    <div class="fav-scroll">
      <template v-if="visibleTree.length > 0">
        <FavoriteTreeNode v-for="node in visibleTree" :key="node.node_id" :node="node" :depth="0" />
      </template>
      <p v-else class="fav-empty">
        {{ searching ? $t('misc.no_search_results') : $t('favorite.no_favorites') }}
      </p>
    </div>

    <KDialog v-model:open="addDialogOpen" :title="$t('favorite.choose_game_add')" :width="560">
      <div class="add-list">
        <div v-for="game in config?.games ?? []" :key="game.name" class="add-row">
          <div class="add-info">
            <span class="add-name">{{ game.name }}</span>
            <span class="add-path">{{ gamePathsText(game) }}</span>
          </div>
          <KButton size="sm" :disabled="favoriteNames.has(game.name)" @click="addGame(game)">
            {{ $t('favorite.add_to_favorite') }}
          </KButton>
        </div>
      </div>
      <template #footer>
        <KButton variant="primary" @click="addAllGames">
          {{ $t('favorite.add_all_games') }}
        </KButton>
      </template>
    </KDialog>
  </div>
</template>

<style scoped>
.fav-tree {
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.fav-actions {
  display: flex;
  gap: 4px;
  padding: 0 8px 6px;
}

.fav-scroll {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.fav-empty {
  margin: 16px 8px;
  text-align: center;
  font-size: 12px;
  color: var(--text-dim);
}

.add-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: 50vh;
  overflow-y: auto;
}

.add-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 6px;
  border-radius: var(--radius-sm);
}

.add-row:hover {
  background-color: var(--surface-2);
}

.add-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.add-name {
  font-size: 13px;
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.add-path {
  font-family: var(--font-mono-stack);
  font-size: 11px;
  color: var(--text-dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
