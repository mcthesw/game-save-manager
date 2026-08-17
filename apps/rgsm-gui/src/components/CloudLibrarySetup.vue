<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { commands, type CloudLibraryStatus } from '~/api/commands';
import { $t } from '~/i18n';
import { KButton } from '../ui/kit';

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
const resetting = ref(false);

const requiresAction = computed(
  () =>
    props.enabled &&
    !props.dirty &&
    !inspecting.value &&
    !initializing.value &&
    (inspectionFailed.value || status.value?.kind === 'empty')
);

watch(
  () => initializing.value,
  (busy) => emit('busy', busy),
  { immediate: true }
);

const statusText = computed(() => {
  if (inspectionFailed.value) return $t('sync_settings.library.inspect_failed');
  const current = status.value;
  if (!current) return '';
  switch (current.kind) {
    case 'join_required':
    case 'cutover_required':
    case 'active':
      return '';
    case 'empty':
      return $t('sync_settings.library.create_failed');
  }
  return '';
});

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
    const result = await commands.inspectCloudLibrary();
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

async function reset() {
  resetting.value = true;
  try {
    const result = await commands.resetBrokenCloudLibrary();
    if (result.status === 'error') {
      notifyError(`${$t('sync_settings.library.reset_failed')}: ${result.error}`);
      return;
    }
    notifySuccess($t('sync_settings.library.reset_success'));
    inspectionFailed.value = false;
    updateStatus(null);
    const fresh = await inspect({ createWhenEmpty: true });
    if (fresh) {
      updateStatus(fresh);
    }
  } finally {
    resetting.value = false;
  }
}

defineExpose({ inspect, create, reset });
</script>
<template>
  <section
    v-if="requiresAction"
    class="flex flex-wrap items-center justify-between gap-4 rounded-md border border-[color-mix(in_oklab,var(--warning)_38%,transparent)] bg-[color-mix(in_oklab,var(--warning)_10%,transparent)] px-5 py-4"
  >
    <div class="min-w-0">
      <p class="mb-1 text-xs font-bold tracking-wide text-warning">
        {{ $t('sync_settings.library.title') }}
      </p>
      <p class="text-sm font-medium leading-relaxed text-text">{{ statusText }}</p>
    </div>
    <div class="flex shrink-0 gap-2">
      <KButton v-if="inspectionFailed" variant="primary" size="sm" @click="inspect()">
        {{ $t('sync_settings.library.inspect') }}
      </KButton>
      <KButton
        v-if="inspectionFailed"
        variant="default"
        size="sm"
        :loading="resetting"
        @click="reset()"
      >
        {{ $t('sync_settings.library.reset') }}
      </KButton>
      <KButton
        v-else-if="status?.kind === 'empty'"
        variant="primary"
        size="sm"
        :loading="initializing"
        @click="create()"
      >
        {{ $t('sync_settings.library.retry_create') }}
      </KButton>
    </div>
  </section>
</template>
