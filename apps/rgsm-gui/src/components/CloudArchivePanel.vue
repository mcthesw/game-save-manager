<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';

import { commands, type CloudArchiveLibraryView } from '../bindings';
import { notifyError, notifyInfo, notifySuccess } from '../composables/useActivityCenter';
import { $t } from '../i18n';
import { formatCloudArchiveBytes as formatBytes } from '../utils/cloudArchivePresentation';

const emit = defineEmits<{
  loaded: [library: CloudArchiveLibraryView];
}>();

const feedback = useFeedback();
const library = ref<CloudArchiveLibraryView | null>(null);
const loading = ref(false);
const materializing = ref(false);

const downloadableSnapshots = computed(
  () =>
    library.value?.games.flatMap((game) =>
      game.snapshots.filter((snapshot) => snapshot.cloud_verified)
    ) ?? []
);
const totalSnapshots = computed(() => downloadableSnapshots.value.length);
const localSnapshots = computed(
  () => downloadableSnapshots.value.filter((snapshot) => snapshot.local_verified).length
);

async function load() {
  loading.value = true;
  try {
    const result = await commands.getCloudArchiveLibrary();
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.load_failed'), result.error);
      return;
    }
    library.value = result.data;
    emit('loaded', result.data);
  } finally {
    loading.value = false;
  }
}

async function materializeAll() {
  const preview = await commands.previewMaterializeAll();
  if (preview.status === 'error') {
    notifyError($t('sync_settings.archives.preview_failed'), preview.error);
    return;
  }
  if (preview.data.snapshot_count === 0) {
    notifyInfo($t('sync_settings.archives.all_local'));
    return;
  }
  try {
    await feedback.confirm(
      $t('sync_settings.archives.download_all_confirm', {
        count: preview.data.snapshot_count,
        size: formatBytes(preview.data.total_bytes),
      }),
      $t('sync_settings.archives.download_all'),
      {
        confirmButtonText: $t('sync_settings.archives.download_all'),
        cancelButtonText: $t('sync_settings.cancel'),
        type: 'info',
      }
    );
  } catch {
    return;
  }
  materializing.value = true;
  try {
    const result = await commands.materializeAllCloudArchives();
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.download_all_failed'), result.error);
      return;
    }
    notifySuccess(
      $t('sync_settings.archives.download_all_success', { count: result.data.downloaded })
    );
    await load();
  } finally {
    materializing.value = false;
  }
}

defineExpose({ load });
onMounted(load);
</script>

<template>
  <section v-loading="loading" class="fleet-status">
    <CloudArchiveToolbar
      :local-snapshots="localSnapshots"
      :total-snapshots="totalSnapshots"
      :materializing="materializing"
      :pending-materialization="library?.pending_materialization ?? false"
      @download-all="materializeAll"
    />
    <ElAlert
      v-if="library?.pending_materialization"
      type="info"
      :closable="false"
      show-icon
      :title="$t('sync_settings.archives.resume_hint')"
      class="resume-alert"
    />
    <CloudDeletedGamesPanel @updated="load" />
  </section>
</template>

<style scoped>
.fleet-status {
  display: grid;
  gap: 12px;
  margin-bottom: 18px;
}

.resume-alert {
  margin: 0;
}
</style>
