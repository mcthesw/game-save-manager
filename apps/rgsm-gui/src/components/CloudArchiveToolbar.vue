<script setup lang="ts">
import { Download } from '@lucide/vue';

import { $t } from '../i18n';
import { KButton } from '../ui/kit';

defineProps<{
  localSnapshots: number;
  totalSnapshots: number;
  materializing: boolean;
  pendingMaterialization: boolean;
}>();

defineEmits<{
  downloadAll: [];
}>();
</script>

<template>
  <section class="flex flex-wrap items-center justify-between gap-4 border-b border-border pb-4">
    <div class="min-w-0">
      <h3 class="text-sm font-semibold text-text">{{ $t('sync_settings.archives.title') }}</h3>
      <p class="mt-1 text-xs leading-relaxed text-text-dim">
        {{ $t('sync_settings.archives.purpose') }}
      </p>
      <p class="mt-1 text-xs text-text-dim">
        {{
          $t('sync_settings.archives.summary', {
            local: localSnapshots,
            total: totalSnapshots,
          })
        }}
      </p>
    </div>
    <KButton variant="primary" :loading="materializing" @click="$emit('downloadAll')">
      <template #icon><Download :size="14" aria-hidden="true" /></template>
      {{
        pendingMaterialization
          ? $t('sync_settings.archives.resume_download')
          : $t('sync_settings.archives.download_all')
      }}
    </KButton>
  </section>
</template>
