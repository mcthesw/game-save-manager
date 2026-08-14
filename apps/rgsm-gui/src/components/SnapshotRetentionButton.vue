<script setup lang="ts">
import { Lock, Unlock } from '@element-plus/icons-vue';
import { ref } from 'vue';

import { commands, type CloudArchiveSnapshotView } from '../api/commands';
import { notifyError, notifySuccess } from '../composables/useActivityCenter';
import { $t } from '../i18n';

const props = defineProps<{
  gameId: string;
  snapshot: CloudArchiveSnapshotView;
}>();
const emit = defineEmits<{ updated: [] }>();
const feedback = useFeedback();
const saving = ref(false);

async function toggleProtection() {
  const protectedValue = !props.snapshot.retention_protected;
  if (!protectedValue) {
    try {
      await feedback.confirm(
        $t('sync_settings.archives.retention.unprotect_confirm'),
        $t('sync_settings.archives.retention.unprotect_title'),
        {
          confirmButtonText: $t('sync_settings.archives.retention.unprotect'),
          cancelButtonText: $t('sync_settings.cancel'),
          type: 'warning',
        }
      );
    } catch {
      return;
    }
  }
  saving.value = true;
  try {
    const result = await commands.setSnapshotRetentionProtected(
      props.gameId,
      props.snapshot.snapshot_id,
      protectedValue,
      !protectedValue
    );
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.retention.protection_failed'), result.error);
      return;
    }
    notifySuccess(
      protectedValue
        ? $t('sync_settings.archives.retention.protected')
        : $t('sync_settings.archives.retention.unprotected')
    );
    emit('updated');
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <ElButton
    :icon="snapshot.retention_protected ? Lock : Unlock"
    text
    :type="snapshot.retention_protected ? 'primary' : 'default'"
    :loading="saving"
    @click="toggleProtection"
  >
    {{
      snapshot.retention_protected
        ? $t('sync_settings.archives.retention.protected_label')
        : $t('sync_settings.archives.retention.protect')
    }}
  </ElButton>
</template>
