<script setup lang="ts">
import { DeleteFilled } from '@element-plus/icons-vue';

import { commands } from '../bindings';
import { $t } from '../i18n';
import { notifyError, notifySuccess } from '../composables/useActivityCenter';

const props = defineProps<{
  gameId: string;
  snapshotId: string;
  label: string;
}>();
const emit = defineEmits<{ updated: [] }>();
const feedback = useFeedback();
const busy = ref(false);

async function evict() {
  try {
    await feedback.confirm(
      $t('sync_settings.archives.evict.confirm', { snapshot: props.label }),
      $t('sync_settings.archives.evict.title'),
      {
        confirmButtonText: $t('sync_settings.archives.evict.action'),
        cancelButtonText: $t('sync_settings.cancel'),
        type: 'warning',
      }
    );
  } catch {
    return;
  }
  busy.value = true;
  try {
    const result = await commands.evictLocalArchive(props.gameId, props.snapshotId, true);
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.evict.failed'), result.error);
      return;
    }
    notifySuccess($t('sync_settings.archives.evict.success'));
    emit('updated');
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <ElButton :icon="DeleteFilled" text :loading="busy" @click="evict">
    {{ $t('sync_settings.archives.evict.action') }}
  </ElButton>
</template>
