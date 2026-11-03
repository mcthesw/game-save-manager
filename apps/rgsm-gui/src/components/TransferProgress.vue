<script setup lang="ts">
import { computed } from 'vue';
import type { TransferProgress } from '../api/generated';
import { $t } from '../i18n';
import { formatCloudArchiveBytes } from '../utils/cloudArchivePresentation';

const props = defineProps<{ progress: TransferProgress; label: string }>();

const summary = computed(() =>
  $t('cloud_sync.transfer_progress', {
    done: Math.min(props.progress.items_done + 1, props.progress.items_total),
    total: props.progress.items_total,
    bytes: props.progress.bytes_done ? formatCloudArchiveBytes(props.progress.bytes_done) : '0 B',
    bytesTotal: formatCloudArchiveBytes(props.progress.bytes_total),
  })
);
</script>

<template>
  <div class="mt-1.5">
    <progress
      :value="progress.bytes_done"
      :max="Math.max(progress.bytes_total, 1)"
      :aria-label="label"
      class="block h-1 w-full accent-accent"
    />
    <div class="mt-1 text-xs tabular-nums text-text-dim">{{ summary }}</div>
  </div>
</template>
