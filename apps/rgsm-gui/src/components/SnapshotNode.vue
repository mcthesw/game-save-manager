<script setup lang="ts">
import { computed } from 'vue';
import { Handle, Position } from '@vue-flow/core';
import type { Snapshot } from '../bindings';
import { $t } from '../i18n';
import dayjs from 'dayjs';
import { Delete, Edit, Flag, Scissor, Share, VideoPlay } from '@element-plus/icons-vue';

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
    <Handle type="target" :position="Position.Top" class="handle-target" />

    <el-popover placement="right" :width="220" trigger="click" popper-class="snapshot-popover">
      <template #reference>
        <div class="node-content">
          <div class="node-header">
            <span class="node-date" :title="fullDate">{{ formattedDate }}</span>
            <div v-if="data.headMarkers.length" class="head-tags">
              <el-tag
                v-for="marker in visibleHeadMarkers"
                :key="marker.deviceId"
                size="small"
                :type="marker.isCurrentDevice ? 'success' : 'info'"
                class="head-tag"
                :title="marker.tooltip"
              >
                {{ marker.label }}
              </el-tag>
              <el-tag v-if="overflowHeadCount > 0" size="small" type="info" class="head-tag">
                +{{ overflowHeadCount }}
              </el-tag>
            </div>
          </div>
          <div class="node-description" :title="description">
            {{ truncatedDescription }}
          </div>
          <div v-if="data.snapshot.size" class="node-size">
            {{ formatFileSize(data.snapshot.size) }}
          </div>
        </div>
      </template>

      <div class="popover-content">
        <div class="popover-header">
          <div class="popover-title">{{ fullDate }}</div>
          <div class="popover-description">{{ description }}</div>
          <div v-if="data.headMarkers.length" class="popover-heads">
            <el-tag
              v-for="marker in data.headMarkers"
              :key="marker.deviceId"
              size="small"
              :type="marker.isCurrentDevice ? 'success' : 'info'"
              :title="marker.tooltip"
            >
              {{ marker.label }}
            </el-tag>
          </div>
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
            :disabled="data.canEditDescription === false"
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
            v-if="!data.isCurrentHead"
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
  max-width: 220px;
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
  border-color: var(--el-color-info-light-5);
}

.snapshot-node.is-current-head {
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
  flex-direction: column;
  align-items: flex-start;
  gap: 6px;
}

.node-date {
  font-size: 13px;
  font-weight: 600;
  color: var(--el-text-color-primary);
  font-family: var(--el-font-family-monospace);
}

.head-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.head-tag {
  transform: scale(0.92);
  font-weight: 600;
  margin: 0;
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

.popover-heads {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 8px;
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
