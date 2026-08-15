<script setup lang="ts">
import { computed } from 'vue';

const props = withDefaults(
  defineProps<{
    tone?: 'neutral' | 'accent' | 'success' | 'warning' | 'danger';
  }>(),
  { tone: 'neutral' }
);

const classes = computed(() => {
  const tones = {
    neutral: 'bg-surface-2 text-text-dim',
    accent: 'bg-accent-soft text-accent',
    success: 'bg-[color-mix(in_oklab,var(--success)_15%,transparent)] text-success',
    warning: 'bg-[color-mix(in_oklab,var(--warning)_15%,transparent)] text-warning',
    danger: 'bg-danger-soft text-danger',
  } as const;
  return [
    'inline-flex h-5 shrink-0 items-center gap-1 rounded-sm px-1.5 text-xs font-medium',
    tones[props.tone],
  ];
});
</script>

<template>
  <span :class="classes"><slot /></span>
</template>
