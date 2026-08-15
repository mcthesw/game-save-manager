<script setup lang="ts">
import { RefreshCw, Trash2 } from '@lucide/vue';

import { commands, type CloudDeviceProfileView } from '../api/commands';
import { $t } from '../i18n';
import { notifyError, notifySuccess } from '../composables/useActivityCenter';
import { KButton, KTag } from '../ui/kit';

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
  <section>
    <div class="flex items-start justify-between gap-4">
      <div class="min-w-0">
        <h3 class="text-sm font-medium text-text">
          {{ $t('sync_settings.archives.profiles.title') }}
        </h3>
        <p class="mt-1 text-xs leading-relaxed text-text-dim">
          {{ $t('sync_settings.archives.profiles.description') }}
        </p>
      </div>
      <KButton
        variant="ghost"
        size="sm"
        :aria-label="$t('common.refresh')"
        :loading="loading"
        @click="load"
      >
        <template #icon><RefreshCw :size="13" aria-hidden="true" /></template>
      </KButton>
    </div>
    <div class="mt-3 flex flex-col">
      <div
        v-for="profile in profiles"
        :key="profile.device_id"
        class="flex items-center justify-between gap-3 border-t border-border py-2.5"
      >
        <div class="min-w-0">
          <div class="truncate text-sm font-medium text-text">{{ profile.name }}</div>
          <div class="truncate font-mono text-[11px] text-text-dim">{{ profile.device_id }}</div>
        </div>
        <div class="flex shrink-0 flex-wrap items-center justify-end gap-1.5">
          <KTag v-if="profile.current" tone="accent">
            {{ $t('sync_settings.archives.profiles.current') }}
          </KTag>
          <KTag v-if="profile.deleted">
            {{
              profile.deletion_incomplete
                ? $t('sync_settings.archives.profiles.incomplete')
                : $t('sync_settings.archives.profiles.removed')
            }}
          </KTag>
          <KTag v-if="profile.head_count > 0">
            {{ $t('sync_settings.archives.profiles.heads', { count: profile.head_count }) }}
          </KTag>
          <KButton
            v-if="!profile.current && (!profile.deleted || profile.deletion_incomplete)"
            variant="ghost"
            size="sm"
            class="text-danger"
            :loading="removing === profile.device_id"
            @click="remove(profile)"
          >
            <template #icon>
              <RefreshCw v-if="profile.deleted" :size="13" aria-hidden="true" />
              <Trash2 v-else :size="13" aria-hidden="true" />
            </template>
            {{
              profile.deleted
                ? $t('sync_settings.archives.profiles.retry')
                : $t('sync_settings.archives.profiles.remove_action')
            }}
          </KButton>
        </div>
      </div>
    </div>
  </section>
</template>
