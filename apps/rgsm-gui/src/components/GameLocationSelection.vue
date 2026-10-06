<script setup lang="ts">
import { computed } from 'vue';
import type { Device, GameDeviceBinding } from '../api/commands';
import { $t } from '../i18n';
import { usePathResolution } from '../composables/usePathResolution';
import ResourceMultiSelect from './ResourceMultiSelect.vue';

const props = defineProps<{
  device?: Device | null;
  modelValue: GameDeviceBinding;
  paths: string[];
  hideAccounts?: boolean;
}>();
const emit = defineEmits<{ 'update:modelValue': [value: GameDeviceBinding] }>();
const { resourceLabel } = usePathResolution();
const selectors = computed(() => {
  const paths = props.paths.join('\n');
  return [
    { kind: 'gameRoot', key: 'rootIds', token: /<(root|base|game)>/, label: 'game_root' },
    { kind: 'storeAccount', key: 'accountIds', token: /<storeuserid>/i, label: 'store_account' },
    {
      kind: 'gameInstallation',
      key: 'installationIds',
      token: /<(base|game)>/,
      label: 'game_installation',
    },
  ].flatMap((selector) => {
    if (props.hideAccounts && selector.kind === 'storeAccount') return [];
    const resources =
      props.device?.resources?.filter((item) => item.kind.type === selector.kind) ?? [];
    return selector.token.test(paths) && resources.length > 1
      ? [
          {
            ...selector,
            options: resources.map((item) => ({ value: item.id, label: resourceLabel(item) })),
          },
        ]
      : [];
  });
});
type SelectionKey = 'rootIds' | 'accountIds' | 'installationIds';
function update(key: string, ids: number[]) {
  emit('update:modelValue', { ...props.modelValue, [key]: ids.length ? ids : null });
}
</script>

<template>
  <div v-if="selectors.length" class="flex flex-col gap-2">
    <div v-for="selector in selectors" :key="selector.key" class="min-w-0">
      <div class="mb-1.5 text-sm font-medium text-text">
        {{ $t(`save_location_drawer.${selector.label}`) }}
      </div>
      <ResourceMultiSelect
        :model-value="modelValue[selector.key as SelectionKey] ?? []"
        :options="selector.options"
        :placeholder="$t(`save_location_drawer.${selector.label}`)"
        @update:model-value="update(selector.key, $event)"
      />
    </div>
  </div>
</template>
