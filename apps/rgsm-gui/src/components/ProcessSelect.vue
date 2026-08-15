<script setup lang="ts">
import { RefreshCw } from '@lucide/vue';
import type { RunningProcessOption } from '../api/commands';
import { $t } from '../i18n';
import { KButton, KInput, KTooltip } from '../ui/kit';

defineProps<{
  modelValue: string;
  options: RunningProcessOption[];
  loading?: boolean;
  placeholder?: string;
}>();

const emit = defineEmits<{
  (event: 'update:modelValue', value: string): void;
  (event: 'refresh'): void;
}>();

// datalist = native combobox: type freely (allow-create) with suggestions (filterable).
const listId = `process-options-${Math.random().toString(36).slice(2, 8)}`;
</script>

<template>
  <div class="flex min-w-0 items-center gap-2">
    <KInput
      :model-value="modelValue"
      class="flex-1"
      :list="listId"
      :placeholder="placeholder"
      :aria-label="placeholder"
      mono
      @update:model-value="emit('update:modelValue', String($event ?? ''))"
    />
    <datalist :id="listId">
      <option v-for="process in options" :key="process.name" :value="process.name">
        {{ process.label }}
      </option>
    </datalist>
    <KTooltip :content="$t('manage.refresh_targets')">
      <KButton
        variant="ghost"
        size="sm"
        :aria-label="$t('manage.refresh_targets')"
        :loading="loading"
        @click="emit('refresh')"
      >
        <RefreshCw :size="14" aria-hidden="true" />
      </KButton>
    </KTooltip>
  </div>
</template>
