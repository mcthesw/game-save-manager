<script setup lang="ts">
import { computed } from 'vue';

const props = withDefaults(
  defineProps<{
    placeholder?: string;
    disabled?: boolean;
    /** Monospace face for data (paths, tokens, timestamps). */
    mono?: boolean;
    size?: 'sm' | 'md';
    type?: string;
    ariaLabel?: string;
  }>(),
  {
    placeholder: undefined,
    disabled: false,
    mono: false,
    size: 'md',
    type: 'text',
    ariaLabel: undefined,
  }
);

const model = defineModel<string>();

const classes = computed(() => [
  'box-border w-full rounded-sm border border-border bg-surface px-3 text-text transition-colors duration-150',
  'placeholder:text-text-dim focus:border-accent focus:outline-none',
  'disabled:cursor-not-allowed disabled:opacity-50',
  props.mono ? 'font-mono' : '',
  props.size === 'sm' ? 'h-7 text-xs' : 'h-9 text-sm',
]);
</script>

<template>
  <input
    v-model="model"
    :type="type"
    :class="classes"
    :placeholder="placeholder"
    :disabled="disabled"
    :aria-label="ariaLabel"
  />
</template>
