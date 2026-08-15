<script setup lang="ts">
import { CheckboxIndicator, CheckboxRoot } from 'reka-ui';
import { Check, Minus } from '@lucide/vue';

withDefaults(
  defineProps<{
    /** Accessible label when no default slot text is given. */
    ariaLabel?: string;
    disabled?: boolean;
  }>(),
  { ariaLabel: undefined, disabled: false }
);

const checked = defineModel<boolean | 'indeterminate'>({ required: true });
</script>

<template>
  <label
    class="inline-flex cursor-pointer select-none items-center gap-2 text-sm text-text"
    :class="{ 'cursor-not-allowed opacity-50': disabled }"
  >
    <CheckboxRoot
      v-model="checked"
      :disabled="disabled"
      :aria-label="ariaLabel"
      class="inline-flex h-4 w-4 shrink-0 cursor-pointer items-center justify-center rounded-[4px] border border-border-strong bg-surface transition-colors duration-150 hover:border-accent focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent disabled:cursor-not-allowed data-[state=checked]:border-accent data-[state=checked]:bg-accent data-[state=indeterminate]:border-accent data-[state=indeterminate]:bg-accent"
    >
      <CheckboxIndicator class="flex items-center justify-center text-accent-contrast">
        <Minus v-if="checked === 'indeterminate'" :size="12" aria-hidden="true" />
        <Check v-else :size="12" aria-hidden="true" />
      </CheckboxIndicator>
    </CheckboxRoot>
    <span v-if="$slots.default" class="leading-none"><slot /></span>
  </label>
</template>
