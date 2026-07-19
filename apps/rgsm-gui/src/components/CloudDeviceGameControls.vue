<script setup lang="ts">
import type { CloudArchiveGameView } from '../bindings';
import { commands } from '../bindings';
import { $t } from '../i18n';
import { notifyError, notifySuccess } from '../composables/useActivityCenter';

const props = defineProps<{ game: CloudArchiveGameView }>();
const emit = defineEmits<{ updated: [] }>();
const feedback = useFeedback();
const busy = ref(false);

async function setVisible(visible: boolean) {
  busy.value = true;
  try {
    const result = await commands.setDeviceGameVisibility(props.game.game_id, visible);
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.device.visibility_failed'), result.error);
      return;
    }
    notifySuccess($t('sync_settings.archives.device.visibility_saved'));
    emit('updated');
  } finally {
    busy.value = false;
  }
}

async function setManaged(managed: boolean) {
  if (!managed) {
    try {
      await feedback.confirm(
        $t('sync_settings.archives.device.stop_confirm'),
        $t('sync_settings.archives.device.stop_title'),
        {
          confirmButtonText: $t('sync_settings.archives.device.stop_action'),
          cancelButtonText: $t('sync_settings.cancel'),
          type: 'warning',
        }
      );
    } catch {
      return;
    }
  }
  busy.value = true;
  try {
    const result = await commands.setDeviceGameManaged(props.game.game_id, managed, !managed);
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.device.management_failed'), result.error);
      return;
    }
    notifySuccess(
      managed
        ? $t('sync_settings.archives.device.manage_success')
        : $t('sync_settings.archives.device.stop_success')
    );
    emit('updated');
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="device-controls" @click.stop>
    <template v-if="game.managed">
      <ElSwitch
        :model-value="game.visible"
        :loading="busy"
        :active-text="$t('sync_settings.archives.device.visible')"
        :inactive-text="$t('sync_settings.archives.device.hidden')"
        inline-prompt
        @change="setVisible(Boolean($event))"
      />
      <ElButton text type="danger" :loading="busy" @click="setManaged(false)">
        {{ $t('sync_settings.archives.device.stop_action') }}
      </ElButton>
    </template>
    <template v-else>
      <ElTag type="info" effect="plain">
        {{ $t('sync_settings.archives.device.not_managed') }}
      </ElTag>
      <ElButton text type="primary" :loading="busy" @click="setManaged(true)">
        {{ $t('sync_settings.archives.device.manage_action') }}
      </ElButton>
    </template>
  </div>
</template>

<style scoped>
.device-controls {
  display: flex;
  align-items: center;
  gap: 6px;
}
</style>
