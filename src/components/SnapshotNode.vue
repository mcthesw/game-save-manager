<script lang="ts" setup>
import { computed } from 'vue';
import { Handle, Position } from '@vue-flow/core';
import { ElButton, ElPopover, ElTag, ElIcon } from 'element-plus';
import {
  RefreshLeft,
  Delete,
  EditPen,
  Scissor,
  PriceTag
} from '@element-plus/icons-vue';
import { $t } from '../i18n';
import type { Snapshot } from '../bindings';

// Props passed by Vue Flow
const props = defineProps<{
  data: {
    snapshot: Snapshot;
    isHead: boolean;
    onRestore: (date: string) => void;
    onDelete: (date: string) => void;
    onEditDescribe: (date: string) => void;
    onDetach: (date: string) => void;
  };
}>();

const snapshot = computed(() => props.data.snapshot);
const isHead = computed(() => props.data.isHead);

// Format date for display
const displayDate = computed(() => {
    // Assuming date is in format YYYY-MM-DD_HH-MM-SS or similar
    // We can just display it as is or format it nicer.
    // The existing table view displays it as is (sortable string).
    return snapshot.value.date.replace(/_/g, ' ');
});

const shortDescribe = computed(() => {
    const desc = snapshot.value.describe;
    if (desc.length > 20) {
        return desc.substring(0, 20) + '...';
    }
    return desc;
});
</script>

<template>
  <div class="snapshot-node" :class="{ 'is-head': isHead }">
    <!-- Target Handle (Bottom) - Inputs from Children (since Bottom-Up means Edge Source=Parent(Bottom) -> Target=Child(Top)) -->
    <!-- Wait, if layout is rankdir BT (Bottom to Top) -->
    <!-- Edges go from Parent -> Child. Parent is below Child. -->
    <!-- So Edge leaves Parent at TOP, enters Child at BOTTOM. -->
    <!-- Source Handle: Top. Target Handle: Bottom. -->
    <Handle type="target" :position="Position.Bottom" class="handle" />

    <el-popover placement="right" :width="240" trigger="click">
      <template #reference>
        <div class="node-content">
          <div class="node-header">
            <span class="date">{{ displayDate }}</span>
            <el-tag v-if="isHead" size="small" type="success" effect="dark">HEAD</el-tag>
          </div>
          <div class="node-body">
            <el-icon><PriceTag /></el-icon>
            <span class="description" :title="snapshot.describe">{{ shortDescribe }}</span>
          </div>
        </div>
      </template>

      <!-- Popover Content (Actions) -->
      <div class="node-actions">
        <el-button size="small" type="primary" :icon="RefreshLeft" @click="data.onRestore(snapshot.date)">
          {{ $t('manage.apply') }}
        </el-button>
        <el-button size="small" :icon="EditPen" @click="data.onEditDescribe(snapshot.date)">
          {{ $t('manage.change_describe') }}
        </el-button>
        <el-button size="small" type="warning" :icon="Scissor" @click="data.onDetach(snapshot.date)">
           {{ $t('manage.detach') }}
        </el-button>
        <el-button size="small" type="danger" :icon="Delete" @click="data.onDelete(snapshot.date)">
          {{ $t('manage.delete') }}
        </el-button>
      </div>
    </el-popover>

    <Handle type="source" :position="Position.Top" class="handle" />
  </div>
</template>

<style scoped>
.snapshot-node {
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color);
  border-radius: 20px;
  padding: 8px 12px;
  min-width: 180px;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
  transition: all 0.2s;
  cursor: pointer;
}

.snapshot-node:hover {
  border-color: var(--el-color-primary);
  box-shadow: 0 4px 8px rgba(0, 0, 0, 0.15);
}

.snapshot-node.is-head {
  border-color: var(--el-color-success);
  border-width: 2px;
  background-color: var(--el-color-success-light-9);
}

.node-content {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.node-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 12px;
  color: var(--el-text-color-secondary);
}

.node-body {
  display: flex;
  align-items: center;
  gap: 4px;
  font-weight: bold;
  font-size: 14px;
  color: var(--el-text-color-primary);
}

.node-actions {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.node-actions .el-button {
  margin-left: 0;
  width: 100%;
}

.handle {
    background-color: var(--el-text-color-secondary);
}
</style>
