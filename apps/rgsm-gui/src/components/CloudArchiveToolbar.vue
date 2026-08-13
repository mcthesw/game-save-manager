<script setup lang="ts">
import { Download } from '@element-plus/icons-vue';

import { $t } from '../i18n';

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
  <section class="copies-row">
    <div>
      <h3>{{ $t('sync_settings.archives.title') }}</h3>
      <p>{{ $t('sync_settings.archives.purpose') }}</p>
      <p class="copies-count">
        {{
          $t('sync_settings.archives.summary', {
            local: localSnapshots,
            total: totalSnapshots,
          })
        }}
      </p>
    </div>
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
  </section>
</template>

<style scoped>
.copies-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 24px;
  padding: 8px 0 18px;
  border-bottom: 1px solid var(--el-border-color-lighter);
}

.copies-row h3 {
  margin: 0 0 4px;
  font-size: 0.95rem;
}

.copies-row p {
  margin: 0;
  color: var(--el-text-color-secondary);
  font-size: 0.82rem;
  line-height: 1.45;
}

.copies-count {
  margin-top: 4px !important;
}

@media (max-width: 640px) {
  .copies-row {
    align-items: flex-start;
    flex-direction: column;
  }
}
</style>
