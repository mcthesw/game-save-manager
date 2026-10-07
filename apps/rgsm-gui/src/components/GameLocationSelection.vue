<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { ChevronDown, FolderOpen } from '@lucide/vue';
import { commands, type Device, type GameDraft, type GameDeviceBinding } from '../api/commands';
import { $t } from '../i18n';
import { KButton, KInput, KMenu, KSelect } from '../ui/kit';
import { notifyError } from '../composables/useActivityCenter';

const props = defineProps<{
  device?: Device | null;
  game: GameDraft;
  modelValue: GameDeviceBinding;
  paths: string[];
  hideAccounts?: boolean;
}>();
const emit = defineEmits<{ 'update:modelValue': [value: GameDeviceBinding] }>();
const locations = ref<{
  roots: { id: string; label: string }[];
  accounts: { id: string; label: string }[];
  installations: string[];
}>({ roots: [], accounts: [], installations: [] });
let request = 0;
watch(
  () => JSON.stringify([props.device, props.game.ludusavi_meta]),
  async () => {
    const current = ++request;
    locations.value = { roots: [], accounts: [], installations: [] };
    if (!props.device) return;
    try {
      const result = await commands.gameLocationOptions(props.game, props.device.id);
      if (current !== request) return;
      if (result.status === 'ok') locations.value = result.data;
      else notifyError(result.error);
    } catch (error) {
      if (current === request) notifyError(String(error));
    }
  },
  { immediate: true }
);
const selectors = computed(() =>
  [
    {
      key: 'rootIds' as const,
      options: locations.value.roots,
      token: /<root>/,
      label: 'game_root',
    },
    {
      key: 'accountIds' as const,
      options: locations.value.accounts,
      token: /<storeuserid>/i,
      label: 'store_account',
    },
  ].filter(
    (s) => !(props.hideAccounts && s.key === 'accountIds') && s.token.test(props.paths.join('\n'))
  )
);
function selectedId(key: 'rootIds' | 'accountIds', options: { id: string }[]): string | undefined {
  const ids = props.modelValue[key];
  return ids?.length === 1
    ? ids[0]
    : ids == null && options.length === 1
      ? options[0]?.id
      : undefined;
}
function update(key: 'rootIds' | 'accountIds', id: string | number | undefined) {
  emit('update:modelValue', { ...props.modelValue, [key]: id === undefined ? null : [String(id)] });
}
const installationPath = computed(
  () =>
    props.modelValue.installationPath ??
    (!props.modelValue.installationIds && locations.value.installations.length === 1
      ? locations.value.installations[0]
      : '') ??
    ''
);
const installations = computed(() =>
  locations.value.installations.map((path) => ({
    type: 'item' as const,
    key: path,
    label: path.replaceAll('\\', '/').split('/').at(-1) || path,
    description: path,
    active: path === installationPath.value,
  }))
);
function setInstallation(path: string | undefined) {
  emit('update:modelValue', {
    ...props.modelValue,
    installationPath: path?.trim() || null,
  });
}
async function chooseInstallation() {
  try {
    const result = await commands.chooseSaveDir();
    if (result.status === 'ok' && result.data) setInstallation(result.data);
    else if (result.status === 'error') notifyError(result.error);
  } catch (error) {
    notifyError(String(error));
  }
}
</script>

<template>
  <div class="flex min-w-0 flex-col gap-3">
    <div v-for="selector in selectors" :key="selector.key" class="min-w-0">
      <div class="mb-1.5 text-sm font-medium text-text">
        {{ $t(`save_location_drawer.${selector.label}`) }}
      </div>
      <KSelect
        :model-value="selectedId(selector.key, selector.options)"
        :options="selector.options.map((o) => ({ value: o.id, label: o.label }))"
        :placeholder="$t(`save_location_drawer.${selector.label}`)"
        :aria-label="$t(`save_location_drawer.${selector.label}`)"
        :title="
          selector.options.find((o) => o.id === selectedId(selector.key, selector.options))?.label
        "
        class="w-full min-w-0 [&>span:first-child]:truncate"
        clearable
        @update:model-value="update(selector.key, $event)"
      />
    </div>
    <div class="min-w-0">
      <div class="mb-1.5 flex items-baseline gap-2 text-sm font-medium text-text">
        {{ $t('save_location_drawer.installation_directory') }}
        <code class="text-xs font-normal text-text-dim">&lt;base&gt;</code>
      </div>
      <div class="flex min-w-0 items-center gap-1">
        <KInput
          :model-value="installationPath"
          class="min-w-0 flex-1"
          :aria-label="$t('save_location_drawer.installation_directory')"
          :placeholder="$t('save_location_drawer.installation_placeholder')"
          :title="installationPath"
          @update:model-value="setInstallation"
        />
        <KMenu
          v-if="installations.length"
          :entries="installations"
          :aria-label="$t('save_location_drawer.detected_installations')"
          @select="setInstallation"
        >
          <KButton variant="ghost" :aria-label="$t('save_location_drawer.detected_installations')"
            ><ChevronDown :size="16"
          /></KButton>
        </KMenu>
        <KButton
          variant="ghost"
          :aria-label="$t('save_location_drawer.choose_installation')"
          :title="$t('save_location_drawer.choose_installation')"
          @click="chooseInstallation"
          ><FolderOpen :size="16"
        /></KButton>
      </div>
    </div>
  </div>
</template>
