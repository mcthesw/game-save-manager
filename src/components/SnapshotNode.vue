<script setup lang="ts">
import { computed } from 'vue';
import { Handle, Position } from '@vue-flow/core';
import type { Snapshot } from '../bindings';
import { $t } from '../i18n';
import dayjs from 'dayjs';
import { VideoPlay, Edit, Delete, Flag, Share, Scissor } from '@element-plus/icons-vue';

const { fromNow } = useRelativeTime();

interface Props {
  data: {
    snapshot: Snapshot;
    isHead: boolean;
    isRoot: boolean;
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
  // Parse date format: YYYY-MM-DD_HH-mm-ss
  const parsed = dayjs(dateStr, 'YYYY-MM-DD_HH-mm-ss');
  if (parsed.isValid()) {
    return parsed.format('MM/DD HH:mm');
  }
  return dateStr;
});

const fullDate = computed(() => {
  const dateStr = props.data.snapshot.date;
  const parsed = dayjs(dateStr, 'YYYY-MM-DD_HH-mm-ss');
  if (parsed.isValid()) {
    return parsed.format('YYYY-MM-DD HH:mm:ss');
  }
  return dateStr;
});

const description = computed(() => {
  return props.data.snapshot.describe || '-';
});

const truncatedDescription = computed(() => {
  const desc = description.value;
  return desc.length > 12 ? desc.slice(0, 12) + '...' : desc;
});

const relativeDate = computed(() => fromNow(props.data.snapshot.date));

function formatFileSize(bytes: number): string {
  if (!bytes || bytes === 0) return '';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), sizes.length - 1);
  // i is guaranteed to be within bounds due to Math.min above
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + sizes[i]!;
}
</script>

<template>
  <div
    class="snapshot-node"
    :class="{
      'is-head': data.isHead,
      'is-root': data.isRoot,
      'is-selected': selected,
    }"
  >
    <!-- Handle for incoming edges (from children above) -->
    <Handle type="target" :position="Position.Top" class="handle-target" />

    <el-popover placement="right" :width="200" trigger="click" popper-class="snapshot-popover">
      <template #reference>
        <div class="node-content">
          <div class="node-header">
            <span class="node-date" :title="fullDate">{{ formattedDate }}</span>
            <el-tag v-if="data.isHead" size="small" type="success" class="head-tag"> HEAD </el-tag>
          </div>
          <div class="node-relative-time">{{ relativeDate }}</div>
          <div class="node-description" :title="description">
            {{ truncatedDescription }}
          </div>
          <div v-if="data.snapshot.size" class="node-size">
            {{ formatFileSize(data.snapshot.size) }}
          </div>
        </div>
      </template>

      <!-- Popover content with actions -->
      <div class="popover-content">
        <div class="popover-header">
          <div class="popover-title">{{ fullDate }}</div>
          <div class="popover-description">{{ description }}</div>
        </div>
        <el-divider style="margin: 8px 0" />
        <div class="popover-actions">
          <el-button
            text
            bg
            size="small"
            type="primary"
            :icon="VideoPlay"
            class="action-btn"
            @click="emit('apply', data.snapshot.date)"
          >
            {{ $t('manage.apply') }}
          </el-button>
          <el-button
            text
            bg
            size="small"
            :icon="Edit"
            class="action-btn"
            @click="emit('changeDescription', data.snapshot.date)"
          >
            {{ $t('manage.change_describe') }}
          </el-button>
          <el-button
            text
            bg
            size="small"
            type="danger"
            :icon="Delete"
            class="action-btn"
            @click="emit('delete', data.snapshot.date)"
          >
            {{ $t('manage.delete') }}
          </el-button>
          <el-divider style="margin: 4px 0" />
          <el-button
            v-if="!data.isHead"
            text
            bg
            size="small"
            type="success"
            :icon="Flag"
            class="action-btn"
            @click="emit('setHead', data.snapshot.date)"
          >
            {{ $t('manage.set_as_head') }}
          </el-button>
          <el-button
            text
            bg
            size="small"
            type="warning"
            :icon="Share"
            class="action-btn"
            @click="emit('createBranch', data.snapshot.date)"
          >
            {{ $t('manage.branch_from_here') }}
          </el-button>
          <el-button
            v-if="!data.isRoot"
            text
            bg
            size="small"
            :icon="Scissor"
            class="action-btn"
            @click="emit('detach', data.snapshot.date)"
          >
            {{ $t('manage.detach') }}
          </el-button>
        </div>
      </div>
    </el-popover>

    <!-- Handle for outgoing edges (to parent below) -->
    <Handle type="source" :position="Position.Bottom" class="handle-source" />
  </div>
</template>

<style scoped>
.snapshot-node {
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color);
  border-radius: 12px;
  padding: 10px 16px;
  min-width: 140px;
  max-width: 200px;
  cursor: pointer;
  transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
  box-shadow: var(--el-box-shadow-light);
}

.snapshot-node:hover {
  border-color: var(--el-color-primary);
  box-shadow: var(--el-box-shadow);
  transform: translateY(-2px);
}

.snapshot-node.is-head {
  border-color: var(--el-color-success);
  background: var(--el-color-success-light-9);
}

.snapshot-node.is-root {
  border-style: dashed;
}

.snapshot-node.is-selected {
  border-color: var(--el-color-primary);
  box-shadow: 0 0 0 2px var(--el-color-primary-light-8);
}

.node-content {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.node-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.node-date {
  font-size: 13px;
  font-weight: 600;
  color: var(--el-text-color-primary);
  font-family: var(--el-font-family-monospace);
}

.head-tag {
  transform: scale(0.9);
  font-weight: bold;
}

.node-relative-time {
  font-size: 11px;
  color: var(--el-color-primary);
  opacity: 0.85;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  line-height: 1.3;
}

.node-description {
  font-size: 12px;
  color: var(--el-text-color-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  line-height: 1.4;
}

.node-size {
  font-size: 11px;
  color: var(--el-text-color-placeholder);
  margin-top: 2px;
}

.handle-target,
.handle-source {
  width: 10px;
  height: 10px;
  background: var(--el-color-primary);
  border: 2px solid var(--el-bg-color);
  transition: all 0.2s;
}

.snapshot-node:hover .handle-target,
.snapshot-node:hover .handle-source {
  width: 12px;
  height: 12px;
}

.handle-target {
  top: -6px;
}

.handle-source {
  bottom: -6px;
}

.popover-content {
  padding: 4px;
}

.popover-header {
  margin-bottom: 8px;
}

.popover-title {
  font-weight: 600;
  color: var(--el-text-color-primary);
  margin-bottom: 4px;
  font-size: 14px;
}

.popover-description {
  font-size: 13px;
  color: var(--el-text-color-secondary);
  word-break: break-word;
  line-height: 1.5;
}

.popover-actions {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.popover-actions .el-button {
  margin: 0;
  width: 100%;
  justify-content: flex-start;
  height: 32px;
}
</style>
