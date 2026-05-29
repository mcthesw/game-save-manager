<script setup lang="ts">
import { RefreshRight } from '@element-plus/icons-vue';
import type { RunningProcessOption } from '../bindings';
import { $t } from '../i18n';

defineProps<{
  modelValue: string;
  options: RunningProcessOption[];
  loading?: boolean;
  placeholder?: string;
}>();

defineEmits<{
  (event: 'update:modelValue', value: string): void;
  (event: 'refresh'): void;
}>();
</script>

<template>
  <div class="process-select">
    <el-select
      :model-value="modelValue"
      filterable
      allow-create
      default-first-option
      size="small"
      class="process-select__input"
      :loading="loading"
      :placeholder="placeholder"
      @update:model-value="$emit('update:modelValue', $event)"
    >
      <el-option
        v-for="process in options"
        :key="process.name"
        :label="process.label"
        :value="process.name"
      />
    </el-select>
    <el-tooltip :content="$t('manage.refresh_targets')" placement="top">
      <el-button
        :icon="RefreshRight"
        size="small"
        circle
        :loading="loading"
        @click="$emit('refresh')"
      />
    </el-tooltip>
  </div>
</template>

<style scoped>
.process-select {
  display: flex;
  min-width: 0;
  gap: 8px;
}

.process-select__input {
  width: 100%;
}
</style>
