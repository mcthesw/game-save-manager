<script setup lang="ts">
import { Download, RefreshCw } from '@lucide/vue';

import { $t } from '../i18n';
import { KButton } from '../ui/kit';

defineProps<{
  localSnapshots: number;
  cloudSnapshots: number;
  totalSnapshots: number;
  loaded: boolean;
  /** Unix milliseconds of the last successful cloud check. */
  checkedAt: number | null;
  refreshing: boolean;
  materializing: boolean;
  pendingMaterialization: boolean;
}>();

defineEmits<{
  downloadAll: [];
  refresh: [];
}>();

function checkedTime(at: number) {
  return new Date(at).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
}
</script>

<template>
  <section class="flex flex-wrap items-center justify-between gap-4 border-b border-border pb-4">
    <div class="min-w-0">
      <h3 class="text-sm font-semibold text-text">{{ $t('sync_settings.archives.title') }}</h3>
      <p class="mt-1 text-xs text-text-dim">
        {{
          loaded
            ? $t('sync_settings.archives.game_summary', {
                local: localSnapshots,
                cloud: cloudSnapshots,
                total: totalSnapshots,
              })
            : $t('sync_settings.archives.game_summary_pending')
        }}
      </p>
    </div>
    <div class="flex items-center gap-3">
      <span v-if="checkedAt" class="text-xs text-text-dim">
        {{ $t('sync_settings.archives.last_checked', { time: checkedTime(checkedAt) }) }}
      </span>
      <KButton
        variant="ghost"
        :aria-label="$t('common.refresh')"
        :loading="refreshing"
        @click="$emit('refresh')"
      >
        <template #icon><RefreshCw :size="14" aria-hidden="true" /></template>
        {{ $t('common.refresh') }}
      </KButton>
      <KButton variant="primary" :loading="materializing" @click="$emit('downloadAll')">
        <template #icon><Download :size="14" aria-hidden="true" /></template>
        {{
          pendingMaterialization
            ? $t('sync_settings.archives.resume_download')
            : $t('sync_settings.archives.download_all')
        }}
      </KButton>
    </div>
  </section>
</template>
