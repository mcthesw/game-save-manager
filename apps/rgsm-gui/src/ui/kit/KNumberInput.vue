<script setup lang="ts">
import { computed } from 'vue';
import KInput from './KInput.vue';

const props = withDefaults(
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
      // 手输绕过原生 step/min/max 约束,这里按 props 收口
      const lo = props.min ?? Number.NEGATIVE_INFINITY;
      const hi = props.max ?? Number.POSITIVE_INFINITY;
      model.value = Math.min(hi, Math.max(lo, parsed));
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
