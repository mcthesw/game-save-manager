<script setup lang="ts">
/**
 * Settings-archetype row: label + optional description left, control right.
 * Rows stack inside a section; the last row drops its hairline via `last:border-b-0`.
 */
withDefaults(
  defineProps<{
    label: string;
    description?: string;
  }>(),
  { description: undefined }
);
</script>

<template>
  <div class="flex items-center justify-between gap-6 border-b border-border py-3 last:border-b-0">
    <div class="min-w-0">
      <div class="text-sm text-text">{{ label }}</div>
      <div v-if="description" class="mt-0.5 text-xs leading-relaxed text-text-dim">
        {{ description }}
      </div>
    </div>
    <div class="shrink-0">
      <slot />
    </div>
  </div>
</template>
