<script setup lang="ts">
import { computed } from 'vue';
import { Handle, Position } from '@vue-flow/core';
import type { Snapshot } from '../api/commands';
import { $t } from '../i18n';
import dayjs from 'dayjs';
import { Flag, GitBranchPlus, Pencil, Play, Scissors, Trash2 } from '@lucide/vue';
import { KPopover, KTag } from '../ui/kit';

interface HeadMarker {
  deviceId: string;
  label: string;
  isCurrentDevice: boolean;
  tooltip: string;
}

interface Props {
  data: {
    snapshot: Snapshot;
    isHead: boolean;
    isCurrentHead: boolean;
    isRoot: boolean;
    headMarkers: HeadMarker[];
    canEditDescription?: boolean;
  };
  selected: boolean;
}

const props = defineProps<Props>();

const emit = defineEmits<{
  apply: [date: string];
  delete: [date: string];
  changeDescription: [date: string];
  setHead: [date: string];
  detach: [date: string];
  createBranch: [date: string];
}>();

const formattedDate = computed(() => {
  const dateStr = props.data.snapshot.date;
  const parsed = dayjs(dateStr, 'YYYY-MM-DD_HH-mm-ss');
  return parsed.isValid() ? parsed.format('MM/DD HH:mm') : dateStr;
});

const fullDate = computed(() => {
  const dateStr = props.data.snapshot.date;
  const parsed = dayjs(dateStr, 'YYYY-MM-DD_HH-mm-ss');
  return parsed.isValid() ? parsed.format('YYYY-MM-DD HH:mm:ss') : dateStr;
});

const description = computed(() => props.data.snapshot.describe || '-');

const truncatedDescription = computed(() => {
  const desc = description.value;
  return desc.length > 12 ? desc.slice(0, 12) + '...' : desc;
});

const visibleHeadMarkers = computed(() => props.data.headMarkers.slice(0, 2));
const overflowHeadCount = computed(() => Math.max(props.data.headMarkers.length - 2, 0));

function formatFileSize(bytes: number): string {
  if (!bytes || bytes === 0) return '';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), sizes.length - 1);
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + sizes[i]!;
}

const actionClass =
  'flex w-full cursor-pointer items-center gap-2 rounded-sm border-none bg-transparent px-2 py-1.5 text-left text-xs text-text transition-colors hover:bg-surface-2 disabled:cursor-not-allowed disabled:opacity-50';
</script>

<template>
  <div
    class="snapshot-node"
    :class="{
      'is-head': data.isHead,
      'is-current-head': data.isCurrentHead,
      'is-root': data.isRoot,
      'is-selected': selected,
    }"
  >
    <Handle type="target" :position="Position.Top" class="node-handle handle-target" />

    <KPopover side="right" align="start" :width="230">
      <div class="flex flex-col gap-1.5">
        <div class="flex flex-col items-start gap-1.5">
          <span class="font-mono text-[13px] font-semibold text-text" :title="fullDate">{{
            formattedDate
          }}</span>
          <div v-if="data.headMarkers.length" class="flex flex-wrap gap-1">
            <KTag
              v-for="marker in visibleHeadMarkers"
              :key="marker.deviceId"
              :tone="marker.isCurrentDevice ? 'accent' : 'neutral'"
              :title="marker.tooltip"
            >
              {{ marker.label }}
            </KTag>
            <KTag v-if="overflowHeadCount > 0">+{{ overflowHeadCount }}</KTag>
          </div>
        </div>
        <div class="truncate text-xs text-text-dim" :title="description">
          {{ truncatedDescription }}
        </div>
        <div v-if="data.snapshot.size" class="font-mono text-[11px] text-text-dim/70">
          {{ formatFileSize(data.snapshot.size) }}
        </div>
      </div>

      <template #content>
        <div class="flex flex-col gap-1">
          <div class="px-1 pb-1">
            <div class="font-mono text-[13px] font-semibold text-text">{{ fullDate }}</div>
            <div class="mt-0.5 break-words text-xs leading-relaxed text-text-dim">
              {{ description }}
            </div>
            <div v-if="data.headMarkers.length" class="mt-1.5 flex flex-wrap gap-1">
              <KTag
                v-for="marker in data.headMarkers"
                :key="marker.deviceId"
                :tone="marker.isCurrentDevice ? 'accent' : 'neutral'"
                :title="marker.tooltip"
              >
                {{ marker.label }}
              </KTag>
            </div>
          </div>
          <div class="h-px bg-border" aria-hidden="true" />
          <button type="button" :class="actionClass" @click="emit('apply', data.snapshot.date)">
            <Play :size="13" aria-hidden="true" />
            {{ $t('manage.apply') }}
          </button>
          <button
            type="button"
            :class="actionClass"
            :disabled="data.canEditDescription === false"
            @click="emit('changeDescription', data.snapshot.date)"
          >
            <Pencil :size="13" aria-hidden="true" />
            {{ $t('manage.change_describe') }}
          </button>
          <button
            type="button"
            :class="[actionClass, 'text-danger']"
            @click="emit('delete', data.snapshot.date)"
          >
            <Trash2 :size="13" aria-hidden="true" />
            {{ $t('manage.delete') }}
          </button>
          <div class="h-px bg-border" aria-hidden="true" />
          <button
            v-if="!data.isCurrentHead"
            type="button"
            :class="actionClass"
            @click="emit('setHead', data.snapshot.date)"
          >
            <Flag :size="13" aria-hidden="true" />
            {{ $t('manage.set_as_head') }}
          </button>
          <button
            type="button"
            :class="actionClass"
            @click="emit('createBranch', data.snapshot.date)"
          >
            <GitBranchPlus :size="13" aria-hidden="true" />
            {{ $t('manage.branch_from_here') }}
          </button>
          <button
            v-if="!data.isRoot"
            type="button"
            :class="actionClass"
            @click="emit('detach', data.snapshot.date)"
          >
            <Scissors :size="13" aria-hidden="true" />
            {{ $t('manage.detach') }}
          </button>
        </div>
      </template>
    </KPopover>

    <Handle type="source" :position="Position.Bottom" class="node-handle handle-source" />
  </div>
</template>

<style scoped>
.snapshot-node {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  padding: 10px 14px;
  min-width: 140px;
  max-width: 220px;
  cursor: pointer;
  transition:
    border-color 0.15s ease,
    box-shadow 0.15s ease;
  box-shadow: 0 1px 3px rgb(0 0 0 / 0.08);
}

.snapshot-node:hover {
  border-color: var(--border-strong);
  box-shadow: 0 4px 12px rgb(0 0 0 / 0.12);
}

.snapshot-node.is-head {
  border-color: var(--border-strong);
}

/* 琥珀 HEAD 钉:当前设备所在快照是分支视图里唯一的强调色 */
.snapshot-node.is-current-head {
  border-color: var(--accent);
  background: var(--accent-soft);
}

.snapshot-node.is-root {
  border-style: dashed;
}

.snapshot-node.is-selected {
  border-color: var(--accent);
  box-shadow: 0 0 0 2px var(--accent-soft);
}

.node-handle {
  width: 9px;
  height: 9px;
  background: var(--border-strong);
  border: 2px solid var(--surface);
  transition:
    width 0.15s ease,
    height 0.15s ease;
}

.is-current-head .node-handle {
  background: var(--accent);
}

.snapshot-node:hover .node-handle {
  width: 11px;
  height: 11px;
}

.handle-target {
  top: -6px;
}

.handle-source {
  bottom: -6px;
}
</style>
