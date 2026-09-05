<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';

import { commands, type CloudArchiveLibraryView } from '../api/commands';
import { notifyError, notifyInfo, notifySuccess } from '../composables/useActivityCenter';
import { $t } from '../i18n';
import { KAlert } from '../ui/kit';
import { formatCloudArchiveBytes as formatBytes } from '../utils/cloudArchivePresentation';

const emit = defineEmits<{
  loaded: [library: CloudArchiveLibraryView];
}>();

const feedback = useFeedback();
const library = ref<CloudArchiveLibraryView | null>(null);
const loading = ref(false);
const materializing = ref(false);

const allSnapshots = computed(() => library.value?.games.flatMap((game) => game.snapshots) ?? []);
const localSnapshots = computed(
  () => allSnapshots.value.filter((snapshot) => snapshot.local_evidence === 'present').length
);
const cloudSnapshots = computed(
  () => allSnapshots.value.filter((snapshot) => snapshot.cloud_verified).length
);
const totalSnapshots = computed(() => allSnapshots.value.length);

async function load(options: { silent?: boolean } = {}) {
  if (loading.value) return;
  loading.value = true;
  try {
    const result = await commands.refreshCloudArchiveLibrary();
    if (result.status === 'error') {
      if (!options.silent) {
        notifyError($t('sync_settings.archives.load_failed'), result.error);
      }
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
let refreshTimer: ReturnType<typeof setInterval> | undefined;
onMounted(() => {
  void load();
  refreshTimer = setInterval(() => {
    void load({ silent: true });
  }, 2000);
});
onUnmounted(() => {
  if (refreshTimer !== undefined) clearInterval(refreshTimer);
});
</script>

<template>
  <section class="mb-4 flex flex-col gap-3">
    <CloudArchiveToolbar
      :local-snapshots="localSnapshots"
      :cloud-snapshots="cloudSnapshots"
      :total-snapshots="totalSnapshots"
      :materializing="materializing"
      :pending-materialization="library?.pending_materialization ?? false"
      @download-all="materializeAll"
    />
    <KAlert v-if="library?.pending_materialization" tone="info">
      {{ $t('sync_settings.archives.resume_hint') }}
    </KAlert>
    <CloudDeletedGamesPanel @updated="load" />
  </section>
</template>
