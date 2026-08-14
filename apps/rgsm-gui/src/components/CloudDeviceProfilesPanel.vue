<script setup lang="ts">
import { Delete, Refresh } from '@element-plus/icons-vue';

import { commands, type CloudDeviceProfileView } from '../api/commands';
import { $t } from '../i18n';
import { notifyError, notifySuccess } from '../composables/useActivityCenter';

const feedback = useFeedback();
const profiles = ref<CloudDeviceProfileView[]>([]);
const loading = ref(false);
const removing = ref('');

async function load() {
  loading.value = true;
  try {
    const result = await commands.getCloudDeviceProfiles();
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.profiles.load_failed'), result.error);
      return;
    }
    profiles.value = result.data;
  } finally {
    loading.value = false;
  }
}

async function remove(profile: CloudDeviceProfileView) {
  if (!profile.deleted) {
    try {
      await feedback.confirm(
        $t('sync_settings.archives.profiles.remove_confirm', { device: profile.name }),
        $t('sync_settings.archives.profiles.remove_title'),
        {
          confirmButtonText: $t('sync_settings.archives.profiles.remove_action'),
          cancelButtonText: $t('sync_settings.cancel'),
          type: 'warning',
        }
      );
    } catch {
      return;
    }
  }
  removing.value = profile.device_id;
  try {
    const result = await commands.removeCloudDeviceProfile(profile.device_id, true);
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.profiles.remove_incomplete'), result.error);
      await load();
      return;
    }
    notifySuccess(
      $t('sync_settings.archives.profiles.remove_success', {
        count: result.data.removed_heads,
      })
    );
    await load();
  } finally {
    removing.value = '';
  }
}

onMounted(load);
</script>

<template>
  <section v-loading="loading" class="profiles-panel">
    <div class="profiles-heading">
      <div>
        <strong>{{ $t('sync_settings.archives.profiles.title') }}</strong>
        <p>{{ $t('sync_settings.archives.profiles.description') }}</p>
      </div>
      <ElButton :icon="Refresh" circle :aria-label="$t('common.refresh')" @click="load" />
    </div>
    <div class="profile-list">
      <div v-for="profile in profiles" :key="profile.device_id" class="profile-row">
        <div class="profile-name">
          <strong>{{ profile.name }}</strong>
          <small>{{ profile.device_id }}</small>
        </div>
        <div class="profile-state">
          <ElTag v-if="profile.current" type="primary" effect="plain">
            {{ $t('sync_settings.archives.profiles.current') }}
          </ElTag>
          <ElTag v-if="profile.deleted" type="info" effect="plain">
            {{
              profile.deletion_incomplete
                ? $t('sync_settings.archives.profiles.incomplete')
                : $t('sync_settings.archives.profiles.removed')
            }}
          </ElTag>
          <ElTag v-if="profile.head_count > 0" effect="plain">
            {{ $t('sync_settings.archives.profiles.heads', { count: profile.head_count }) }}
          </ElTag>
          <ElButton
            v-if="!profile.current && (!profile.deleted || profile.deletion_incomplete)"
            :icon="profile.deleted ? Refresh : Delete"
            text
            type="danger"
            :loading="removing === profile.device_id"
            @click="remove(profile)"
          >
            {{
              profile.deleted
                ? $t('sync_settings.archives.profiles.retry')
                : $t('sync_settings.archives.profiles.remove_action')
            }}
          </ElButton>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.profiles-panel {
  padding: 8px 0 18px;
  border-bottom: 1px solid var(--el-border-color-lighter);
}

.profiles-heading,
.profile-row,
.profile-state {
  display: flex;
  align-items: center;
}

.profiles-heading {
  justify-content: space-between;
  gap: 16px;
}

.profiles-heading p {
  margin: 4px 0 0;
  color: var(--el-text-color-secondary);
  font-size: 0.82rem;
}

.profile-list {
  display: grid;
  margin-top: 10px;
}

.profile-row {
  justify-content: space-between;
  gap: 12px;
  min-height: 44px;
  border-top: 1px solid var(--el-border-color-lighter);
}

.profile-name {
  display: grid;
  min-width: 0;
}

.profile-name small {
  overflow: hidden;
  color: var(--el-text-color-secondary);
  text-overflow: ellipsis;
}

.profile-state {
  justify-content: flex-end;
  gap: 6px;
  flex-wrap: wrap;
}

@media (max-width: 640px) {
  .profile-row {
    align-items: flex-start;
    flex-direction: column;
    padding: 8px 0;
  }

  .profile-state {
    justify-content: flex-start;
  }
}
</style>
