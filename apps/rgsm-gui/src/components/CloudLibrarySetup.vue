<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { Connection } from '@element-plus/icons-vue';
import { commands, type CloudLibraryStatus } from '~/bindings';
import { $t } from '~/i18n';

const props = defineProps<{
  enabled: boolean;
  connectionKey: string;
}>();

const emit = defineEmits<{
  (event: 'status', value: CloudLibraryStatus | null): void;
}>();

const feedback = useFeedback();
const status = ref<CloudLibraryStatus | null>(null);
const inspecting = ref(false);
const creating = ref(false);

const alertType = computed(() => {
  switch (status.value?.kind) {
    case 'active':
      return 'success';
    case 'empty':
      return 'info';
    case 'join_required':
    case 'cutover_required':
      return 'warning';
    default:
      return 'info';
  }
});

const statusText = computed(() => {
  const current = status.value;
  if (!current) return $t('sync_settings.library.not_checked');
  switch (current.kind) {
    case 'empty':
      return $t('sync_settings.library.empty');
    case 'join_required':
      return $t('sync_settings.library.join_required', { count: current.game_count });
    case 'cutover_required':
      return $t('sync_settings.library.cutover_required', { count: current.game_count });
    case 'active':
      return $t('sync_settings.library.active', { count: current.game_count });
  }
  return $t('sync_settings.library.not_checked');
});

function updateStatus(value: CloudLibraryStatus | null) {
  status.value = value;
  emit('status', value);
}

async function inspect() {
  if (!props.enabled || inspecting.value) return;
  inspecting.value = true;
  try {
    const result = await commands.inspectCloudLibrary();
    if (result.status === 'error') {
      updateStatus(null);
      notifyError(`${$t('sync_settings.library.inspect_failed')}: ${result.error}`);
      return;
    }
    updateStatus(result.data);
  } catch (reason) {
    updateStatus(null);
    notifyError(`${$t('sync_settings.library.inspect_failed')}: ${String(reason)}`);
  } finally {
    inspecting.value = false;
  }
}

async function create() {
  try {
    await feedback.confirm(
      $t('sync_settings.library.create_warning'),
      $t('sync_settings.library.create_title'),
      {
        confirmButtonText: $t('sync_settings.library.create_confirm'),
        cancelButtonText: $t('sync_settings.cancel'),
        type: 'warning',
      }
    );
  } catch {
    return;
  }

  creating.value = true;
  try {
    const result = await commands.createCloudLibrary(true);
    if (result.status === 'error') {
      notifyError(`${$t('sync_settings.library.create_failed')}: ${result.error}`);
      await inspect();
      return;
    }
    updateStatus(result.data);
    notifySuccess($t('sync_settings.library.create_success'));
  } catch (reason) {
    notifyError(`${$t('sync_settings.library.create_failed')}: ${String(reason)}`);
  } finally {
    creating.value = false;
  }
}

watch(
  () => [props.enabled, props.connectionKey] as const,
  ([enabled]) => {
    updateStatus(null);
    if (enabled) void inspect();
  },
  { immediate: true }
);
</script>

<template>
  <section class="library-card">
    <div class="library-heading">
      <div>
        <h3>{{ $t('sync_settings.library.title') }}</h3>
        <p>{{ $t('sync_settings.library.description') }}</p>
      </div>
      <ElIcon :size="24"><Connection /></ElIcon>
    </div>

    <ElAlert v-if="enabled" :type="alertType" :title="statusText" :closable="false" show-icon />
    <ElAlert
      v-else
      type="info"
      :title="$t('sync_settings.library.backend_disabled')"
      :closable="false"
      show-icon
    />

    <div class="library-actions">
      <ElButton :disabled="!enabled" :loading="inspecting" @click="inspect">
        {{ $t('sync_settings.library.inspect') }}
      </ElButton>
      <ElButton v-if="status?.kind === 'empty'" type="primary" :loading="creating" @click="create">
        {{ $t('sync_settings.library.create') }}
      </ElButton>
    </div>
  </section>
</template>

<style scoped>
.library-card {
  padding: 20px;
  border: 1px solid var(--el-border-color-light);
  border-radius: var(--el-border-radius-base);
  background: var(--el-fill-color-blank);
}

.library-heading {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 16px;
  color: var(--el-text-color-primary);
}

.library-heading h3 {
  margin: 0 0 6px;
  font-size: 16px;
}

.library-heading p {
  margin: 0;
  color: var(--el-text-color-secondary);
  line-height: 1.5;
}

.library-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  margin-top: 14px;
}
</style>
