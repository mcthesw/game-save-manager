<script setup lang="ts">
import { Download, Refresh } from '@element-plus/icons-vue';

import { $t } from '../i18n';

defineProps<{
  localSnapshots: number;
  totalSnapshots: number;
  materializing: boolean;
  pendingMaterialization: boolean;
}>();

defineEmits<{
  refresh: [];
  downloadAll: [];
}>();
</script>

<template>
  <div class="archive-toolbar">
    <div>
      <h3>{{ $t('sync_settings.archives.title') }}</h3>
      <p>
        {{
          $t('sync_settings.archives.summary', {
            local: localSnapshots,
            total: totalSnapshots,
          })
        }}
      </p>
    </div>
    <div class="toolbar-actions">
      <ElButton
        :icon="Refresh"
        circle
        :aria-label="$t('common.refresh')"
        @click="$emit('refresh')"
      />
      <ElButton
        type="primary"
        :icon="Download"
        :loading="materializing"
        @click="$emit('downloadAll')"
      >
        {{
          pendingMaterialization
            ? $t('sync_settings.archives.resume_download')
            : $t('sync_settings.archives.download_all')
        }}
      </ElButton>
    </div>
  </div>
</template>

<style scoped>
.archive-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 20px;
  margin-bottom: 16px;
}

.archive-toolbar h3 {
  margin: 0 0 4px;
}

.archive-toolbar p {
  margin: 0;
  color: var(--el-text-color-secondary);
  font-size: 0.82rem;
}

.toolbar-actions {
  display: flex;
  gap: 8px;
  flex-shrink: 0;
}

@media (max-width: 640px) {
  .archive-toolbar {
    align-items: flex-start;
    flex-direction: column;
  }
}
</style>
