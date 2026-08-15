<script setup lang="ts">
import { computed } from 'vue';
import KInput from './KInput.vue';

withDefaults(
  defineProps<{
    min?: number;
    max?: number;
    step?: number;
    placeholder?: string;
    disabled?: boolean;
    ariaLabel?: string;
  }>(),
  {
    min: undefined,
    max: undefined,
    step: 1,
    placeholder: undefined,
    disabled: false,
    ariaLabel: undefined,
  }
);

const model = defineModel<number | undefined>();

/** Empty field reads as undefined (unset), not 0 — 0 is a real value here. */
const text = computed<string>({
  get: () => (model.value === undefined || Number.isNaN(model.value) ? '' : String(model.value)),
  set: (raw: string) => {
    if (raw === '') {
      model.value = undefined;
      return;
    }
    const parsed = Number(raw);
    if (!Number.isNaN(parsed)) {
      model.value = parsed;
    }
  },
});
</script>

<template>
  <KInput
    v-model="text"
    type="number"
    mono
    :min="min"
    :max="max"
    :step="step"
    :placeholder="placeholder"
    :disabled="disabled"
    :aria-label="ariaLabel"
  />
</template>
