<script setup lang="ts">
import { ref, watch } from 'vue';
import { commands, type CloudJoinPreview } from '../api/commands';
import { $t } from '../i18n';
import { KAlert, KButton, KDialog, KInput } from '../ui/kit';
import { notifyError, notifySuccess } from '../composables/useActivityCenter';
import { clearCloudLibrary, refreshCloudLibrary } from '../composables/useCloudLibrary';
import { useConfig } from '../composables/useConfig';

defineProps<{ canExport: boolean }>();
const emit = defineEmits<{ connected: [] }>();
const open = ref(false);
const exporting = ref(false);
const busy = ref(false);
const code = ref('');
const preview = ref<CloudJoinPreview | null>(null);
const errorText = ref('');

watch(code, () => {
  preview.value = null;
  errorText.value = '';
});
watch(open, (visible) => {
  if (!visible) {
    code.value = '';
    preview.value = null;
    errorText.value = '';
  }
});

async function start(isExport: boolean) {
  exporting.value = isExport;
  open.value = true;
  if (!isExport) return;
  busy.value = true;
  try {
    const result = await commands.exportCloudJoinCode();
    if (result.status === 'error') errorText.value = result.error;
    else code.value = result.data;
  } catch {
    errorText.value = $t('cloud_join.failed');
  } finally {
    busy.value = false;
  }
}

async function inspect() {
  busy.value = true;
  errorText.value = '';
  try {
    const result = await commands.previewCloudJoinCode(code.value);
    if (result.status === 'error') errorText.value = result.error;
    else preview.value = result.data;
  } catch {
    errorText.value = $t('cloud_join.failed');
  } finally {
    busy.value = false;
  }
}

async function connect() {
  if (!preview.value) return;
  busy.value = true;
  try {
    const result = await commands.importCloudJoinCode(code.value);
    if (result.status === 'error') {
      errorText.value = result.error;
      return;
    }
    await useConfig().refreshConfig();
    clearCloudLibrary();
    await refreshCloudLibrary();
    notifySuccess($t('cloud_join.connected'));
    open.value = false;
    emit('connected');
  } catch {
    errorText.value = $t('cloud_join.failed');
  } finally {
    busy.value = false;
  }
}

async function copy() {
  try {
    await navigator.clipboard.writeText(code.value);
    notifySuccess($t('cloud_join.copied'));
  } catch {
    notifyError($t('cloud_join.copy_failed'));
  }
}
</script>

<template>
  <div class="flex flex-wrap gap-2">
    <KButton @click="start(false)">{{ $t('cloud_join.import') }}</KButton>
    <KButton v-if="canExport" @click="start(true)">{{ $t('cloud_join.export') }}</KButton>
  </div>
  <KDialog
    v-model:open="open"
    :title="$t(exporting ? 'cloud_join.export' : 'cloud_join.import')"
    :dismissable="!busy"
    :width="600"
  >
    <template #description>{{ $t('cloud_join.scope') }}</template>
    <div class="flex flex-col gap-3">
      <KAlert tone="warning">{{ $t('cloud_join.sensitive') }}</KAlert>
      <KInput
        v-model="code"
        class="w-full"
        mono
        :readonly="exporting"
        :disabled="busy"
        :aria-label="$t('cloud_join.code')"
        autocomplete="off"
        spellcheck="false"
      />
      <KAlert v-if="errorText" tone="danger">{{ errorText }}</KAlert>
      <div v-if="preview" class="rounded-sm border border-border p-3 text-sm break-all">
        <div>{{ preview.backend }} · {{ preview.endpoint }}</div>
        <div v-if="preview.bucket">{{ preview.bucket }}</div>
        <div>{{ preview.root_path }}</div>
        <div class="mt-2 text-text-dim">
          {{ $t('cloud_join.games', { count: preview.game_count }) }}
        </div>
        <div class="mt-2">{{ $t('cloud_join.confirm_hint') }}</div>
      </div>
    </div>
    <template #footer>
      <KButton :disabled="busy" @click="open = false">{{ $t('common.cancel') }}</KButton>
      <KButton v-if="exporting" variant="primary" :disabled="busy || !code" @click="copy">{{
        $t('cloud_join.copy')
      }}</KButton>
      <KButton v-else-if="preview" variant="primary" :disabled="busy" @click="connect">{{
        $t('cloud_join.connect')
      }}</KButton>
      <KButton v-else variant="primary" :disabled="busy || !code.trim()" @click="inspect">{{
        $t('cloud_join.preview')
      }}</KButton>
    </template>
  </KDialog>
</template>
