<script lang="ts" setup>
import { computed, inject } from 'vue';
import { ChevronDown, ChevronRight, Folder, X } from '@lucide/vue';
import { $t } from '../i18n';
import type { FavoriteTreeNode } from '../api/commands';
import { FAVORITE_TREE_CTX } from './favoriteTreeContext';

const props = defineProps<{
  node: FavoriteTreeNode;
  depth: number;
}>();

const ctx = inject(FAVORITE_TREE_CTX)!;
// 解构出 ref，模板里才能自动解包
const { editMode } = ctx;

const isOpen = computed(() => ctx.searching.value || ctx.expandedIds.value.has(props.node.node_id));
const dropClass = computed(() => {
  const target = ctx.dropTarget.value;
  return target && target.id === props.node.node_id ? `drop-${target.pos}` : '';
});
</script>

<template>
  <div
    class="fav-row"
    :class="[node.is_leaf ? 'leaf' : 'folder', dropClass]"
    :style="{ paddingLeft: `${8 + depth * 16}px` }"
    :draggable="editMode"
    @click="node.is_leaf ? ctx.clickLeaf(node) : ctx.toggleExpand(node.node_id)"
    @dragstart="ctx.onDragStart(node.node_id, $event)"
    @dragover="ctx.onDragOver(node.node_id, node.is_leaf, $event)"
    @dragleave="ctx.onDragLeave(node.node_id)"
    @drop="ctx.onDrop(node.node_id)"
    @dragend="ctx.onDragEnd()"
  >
    <template v-if="!node.is_leaf">
      <component :is="isOpen ? ChevronDown : ChevronRight" :size="13" class="fav-chevron" />
      <Folder :size="13" class="fav-folder-icon" />
    </template>
    <span v-else class="fav-leaf-indent" />
    <span class="fav-label">{{ node.label }}</span>
    <button
      v-if="editMode"
      type="button"
      class="fav-remove"
      :aria-label="$t('favorite.remove')"
      @click.stop="ctx.removeNode(node.node_id)"
    >
      <X :size="12" />
    </button>
  </div>
  <template v-if="!node.is_leaf && isOpen">
    <FavoriteTreeNode
      v-for="child in node.children ?? []"
      :key="child.node_id"
      :node="child"
      :depth="depth + 1"
    />
  </template>
</template>

<style scoped>
.fav-row {
  display: flex;
  align-items: center;
  gap: 4px;
  width: 100%;
  padding: 5px 8px;
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  cursor: pointer;
  font-size: 13px;
  color: var(--text);
  transition: background-color 0.15s;
}

.fav-row:hover {
  background-color: var(--surface-2);
}

.fav-chevron {
  flex-shrink: 0;
  color: var(--text-dim);
}

.fav-folder-icon {
  flex-shrink: 0;
  color: var(--text-dim);
}

.fav-leaf-indent {
  width: 17px;
  flex-shrink: 0;
}

.fav-label {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: left;
}

.fav-remove {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  flex-shrink: 0;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--danger);
  cursor: pointer;
}

.fav-remove:hover {
  background-color: var(--surface);
}

/* 拖拽落点反馈（中性色，不占琥珀） */
.fav-row.drop-before {
  border-top-color: var(--text);
}

.fav-row.drop-after {
  border-bottom-color: var(--text);
}

.fav-row.drop-inner {
  background-color: var(--surface-2);
  border-color: var(--border-strong);
}

.fav-row[draggable='true'] {
  cursor: grab;
}
</style>
