<script setup lang="ts" generic="T extends string">
import type { Component } from 'vue';

export type KSegmentedOption<T extends string = string> = {
  value: T;
  label: string;
  icon?: Component;
};

withDefaults(
  defineProps<{
    options: KSegmentedOption<T>[];
    ariaLabel?: string;
  }>(),
  { ariaLabel: undefined }
);

const model = defineModel<T>({ required: true });
</script>

<template>
  <div
    role="tablist"
    :aria-label="ariaLabel"
    class="flex gap-0.5 rounded-sm border border-border bg-surface p-0.5"
  >
    <button
      v-for="option in options"
      :key="option.value"
      type="button"
      role="tab"
      :aria-selected="model === option.value"
      class="inline-flex flex-1 cursor-pointer items-center justify-center gap-1.5 rounded-[calc(var(--radius-sm)-2px)] border-none bg-transparent px-2.5 py-1 text-xs transition-colors duration-150 hover:text-text focus-visible:outline-2 focus-visible:outline-accent"
      :class="model === option.value ? 'bg-surface-2 font-semibold text-text' : 'text-text-dim'"
      @click="model = option.value"
    >
      <component :is="option.icon" v-if="option.icon" :size="13" aria-hidden="true" />
      {{ option.label }}
    </button>
  </div>
</template>
