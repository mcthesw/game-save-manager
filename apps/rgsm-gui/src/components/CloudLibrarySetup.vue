<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { commands, type CloudLibraryStatus } from '~/api/commands';
import { $t } from '~/i18n';
import { KButton } from '../ui/kit';
import { connectSavedCloudLibrary } from '../composables/useCloudConnection';
import { refreshCloudLibrary } from '../composables/useCloudLibrary';
import { cloudLibraryNotice } from '../utils/cloudLibraryNotice';

interface InspectOptions {
  createWhenEmpty?: boolean;
  silent?: boolean;
}

const props = defineProps<{
  enabled: boolean;
  dirty: boolean;
}>();

const emit = defineEmits<{
  (event: 'status', value: CloudLibraryStatus | null): void;
  (event: 'busy', value: boolean): void;
}>();

const status = ref<CloudLibraryStatus | null>(null);
const inspecting = ref(false);
const initializing = ref(false);
const initializationFailed = ref(false);
const inspectionFailed = ref(false);
const recovering = ref(false);
const feedback = useFeedback();

const notice = computed(() =>
  cloudLibraryNotice(status.value, inspectionFailed.value, initializationFailed.value)
);
const busy = computed(() => inspecting.value || initializing.value || recovering.value);
const requiresAction = computed(() => props.enabled && !props.dirty && notice.value !== null);

watch(busy, (busy) => emit('busy', busy), { immediate: true });

function updateStatus(value: CloudLibraryStatus | null) {
  status.value = value;
  emit('status', value);
}

let inspectionGeneration = 0;
let activeInspection: Promise<CloudLibraryStatus | null> | null = null;

async function create(requestGeneration?: number): Promise<CloudLibraryStatus | null> {
  if (initializing.value) return status.value;
  initializing.value = true;
  initializationFailed.value = false;
  try {
    const result = await commands.createCloudLibrary(true);
    if (
      requestGeneration !== undefined &&
      (requestGeneration !== inspectionGeneration || !props.enabled)
    ) {
      return null;
    }
    if (result.status === 'error') {
      initializationFailed.value = true;
      notifyError(`${$t('sync_settings.library.create_failed')}: ${result.error}`);
      return null;
    }
    updateStatus(result.data);
    notifySuccess($t('sync_settings.library.create_success'));
    return result.data;
  } catch (reason) {
    if (requestGeneration !== undefined && requestGeneration !== inspectionGeneration) {
      return null;
    }
    initializationFailed.value = true;
    notifyError(`${$t('sync_settings.library.create_failed')}: ${String(reason)}`);
    return null;
  } finally {
    initializing.value = false;
  }
}

async function performInspection(
  requestGeneration: number,
  options: InspectOptions
): Promise<CloudLibraryStatus | null> {
  inspecting.value = true;
  inspectionFailed.value = false;
  initializationFailed.value = false;
  try {
    let result = await commands.inspectCloudLibrary();
    if (result.status === 'ok' && result.data.kind === 'join_required') {
      result = await connectSavedCloudLibrary();
    }
    if (requestGeneration !== inspectionGeneration || !props.enabled) return null;
    if (result.status === 'error') {
      inspectionFailed.value = true;
      updateStatus(null);
      notifyError(`${$t('sync_settings.library.inspect_failed')}: ${result.error}`);
      return null;
    }

    updateStatus(result.data);
    if (result.data.kind === 'empty' && options.createWhenEmpty) {
      return await create(requestGeneration);
    }
    return result.data;
  } catch (reason) {
    if (requestGeneration !== inspectionGeneration || !props.enabled) return null;
    inspectionFailed.value = true;
    updateStatus(null);
    notifyError(`${$t('sync_settings.library.inspect_failed')}: ${String(reason)}`);
    return null;
  } finally {
    inspecting.value = false;
  }
}

async function inspect(options: InspectOptions = {}): Promise<CloudLibraryStatus | null> {
  const requestGeneration = ++inspectionGeneration;
  if (!props.enabled) {
    inspectionFailed.value = false;
    initializationFailed.value = false;
    updateStatus(null);
    return null;
  }

  const previousInspection = activeInspection;
  if (previousInspection) await previousInspection;
  if (requestGeneration !== inspectionGeneration || !props.enabled) return null;

  const inspection = performInspection(requestGeneration, options);
  activeInspection = inspection;
  try {
    return await inspection;
  } finally {
    if (activeInspection === inspection) activeInspection = null;
  }
}

async function rebuild() {
  recovering.value = true;
  try {
    await feedback.prompt(
      $t('sync_settings.library.rebuild_confirm'),
      $t('sync_settings.library.rebuild'),
      {
        confirmButtonText: $t('sync_settings.library.rebuild'),
        cancelButtonText: $t('sync_settings.cancel'),
        inputPattern: /yes/,
        inputErrorMessage: $t('manage.invalid_input_error'),
      }
    );
    const result = await commands.rebuildCloudLibraryFromLocal(true);
    if (result.status === 'error') {
      notifyError(`${$t('sync_settings.library.rebuild_failed')}: ${result.error}`);
      return;
    }
    notifySuccess($t('sync_settings.library.rebuild_success'));
    inspectionFailed.value = false;
    updateStatus(result.data);
  } catch {
    // User cancelled the confirmation prompt.
  } finally {
    recovering.value = false;
  }
}

async function reconnect() {
  recovering.value = true;
  try {
    await feedback.confirm(
      $t('sync_settings.library.reconnect_confirm'),
      $t('sync_settings.library.reconnect'),
      {
        confirmButtonText: $t('sync_settings.library.reconnect'),
        cancelButtonText: $t('sync_settings.cancel'),
      }
    );
    const result = await commands.reconnectCloudLibrary(true);
    if (result.status === 'error') {
      notifyError(`${$t('sync_settings.library.reconnect_failed')}: ${result.error}`);
      return;
    }
    notifySuccess($t('sync_settings.library.reconnect_success'));
    await refreshCloudLibrary(true);
    updateStatus(result.data);
  } catch {
    // User cancelled the confirmation dialog.
  } finally {
    recovering.value = false;
  }
}

function runAction() {
  if (busy.value || !requiresAction.value) return;
  switch (notice.value?.action) {
    case 'inspect':
      return inspect();
    case 'create':
      return create();
    case 'rebuild':
      return rebuild();
    case 'reconnect':
      return reconnect();
  }
}

defineExpose({ inspect, create, rebuild, reconnect });
</script>
<template>
  <section
    v-if="requiresAction && notice"
    class="flex flex-wrap items-center justify-between gap-4 rounded-md border border-[color-mix(in_oklab,var(--warning)_38%,transparent)] bg-[color-mix(in_oklab,var(--warning)_10%,transparent)] px-5 py-4"
  >
    <div class="min-w-0">
      <p class="mb-1 text-xs font-bold tracking-wide text-warning">
        {{ $t('sync_settings.library.title') }}
      </p>
      <p class="text-sm font-medium leading-relaxed text-text">{{ $t(notice.messageKey) }}</p>
    </div>
    <KButton variant="primary" size="sm" :loading="busy" @click="runAction">
      {{ $t(notice.actionKey) }}
    </KButton>
  </section>
</template>
