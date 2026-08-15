<script setup lang="ts">
import { computed } from 'vue';
import { CircleAlert, CircleCheck, Info, TriangleAlert } from '@lucide/vue';

const props = withDefaults(
  defineProps<{
    tone?: 'info' | 'success' | 'warning' | 'danger';
  }>(),
  { tone: 'info' }
);

const tones = {
  info: { classes: 'border-accent/30 bg-accent-soft text-text', icon: Info },
  success: { classes: 'border-success/30 bg-success/10 text-text', icon: CircleCheck },
  warning: { classes: 'border-warning/30 bg-warning/10 text-text', icon: TriangleAlert },
  danger: { classes: 'border-danger/30 bg-danger-soft text-text', icon: CircleAlert },
} as const;

const tone = computed(() => tones[props.tone]);
</script>

<template>
  <div
    role="status"
    class="flex items-start gap-2 rounded-sm border px-3 py-2 text-sm leading-relaxed"
    :class="tone.classes"
  >
    <component
      :is="tone.icon"
      :size="15"
      class="mt-0.5 shrink-0 text-text-dim"
      aria-hidden="true"
    />
    <div class="min-w-0"><slot /></div>
  </div>
</template>
