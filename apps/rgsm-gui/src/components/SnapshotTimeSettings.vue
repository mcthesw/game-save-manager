<script setup lang="ts">
import { computed } from 'vue';
import type { SnapshotTimeFormat } from '../api/generated/types.gen';
import { $t } from '../i18n';
import KSelect from '../ui/kit/KSelect.vue';

const model = defineModel<SnapshotTimeFormat>({ default: 'absolute' });
const options = computed(() => [
  { label: $t('settings.snapshot_time_absolute'), value: 'absolute' },
  { label: $t('settings.snapshot_time_relative'), value: 'relative' },
]);
</script>

<template>
  <div class="flex items-center justify-between gap-4 py-1.5">
    <span class="shrink-0 text-sm text-text">{{ $t('settings.snapshot_time_format') }}</span>
    <KSelect
      :model-value="model"
      class="w-56"
      :options="options"
      :aria-label="$t('settings.snapshot_time_format')"
      @update:model-value="model = $event === 'relative' ? 'relative' : 'absolute'"
    />
  </div>
</template>
