<script setup lang="ts">
import type { Game, SavePathOverride, SaveUnit } from '../api/commands';
import { $t } from '../i18n';
import PathVariableInput from './PathVariableInput.vue';
import { KSwitch } from '../ui/kit';

defineProps<{
  unit: SaveUnit;
  game: Game;
  modelValue: string;
  overrideValue?: SavePathOverride;
  local: boolean;
}>();
const emit = defineEmits<{
  'update:modelValue': [path: string];
  'update:overrideValue': [value: SavePathOverride | undefined];
}>();
function setOverride(enabled: boolean) {
  emit('update:overrideValue', enabled === true ? { path: '' } : undefined);
}
</script>

<template>
  <div class="flex min-w-0 flex-col gap-2">
    <div class="flex min-h-7 items-center gap-2">
      <slot name="type" />
      <slot name="identity" />
    </div>
    <PathVariableInput
      :game="game"
      :pattern="unit.source.type === 'manifestPattern' && !overrideValue"
      :model-value="modelValue"
      :status-mode="local ? 'tooltip' : 'none'"
      @update:model-value="emit('update:modelValue', String($event ?? ''))"
    />
    <div class="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
      <div class="flex flex-wrap items-center gap-x-6 gap-y-2">
        <slot name="options" />
        <label
          v-if="unit.source.type === 'manifestPattern'"
          class="inline-flex cursor-pointer items-center gap-2 text-xs text-text-dim"
        >
          <span>{{ $t('save_location_drawer.override_path') }}</span>
          <KSwitch
            :aria-label="$t('save_location_drawer.override_path')"
            :model-value="!!overrideValue"
            @update:model-value="setOverride"
          />
        </label>
      </div>
      <div class="ml-auto flex items-center gap-1">
        <slot name="actions" />
      </div>
    </div>
  </div>
</template>
