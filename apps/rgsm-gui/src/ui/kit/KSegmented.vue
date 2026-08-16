<script setup lang="ts" generic="T extends string">
import type { Component } from 'vue';

export type KSegmentedOption<T extends string = string> = {
  value: T;
  label: string;
  icon?: Component;
};

const props = withDefaults(
  defineProps<{
    options: KSegmentedOption<T>[];
    ariaLabel?: string;
  }>(),
  { ariaLabel: undefined }
);

const model = defineModel<T>({ required: true });

/** Roving selection: arrows/Home/End move both selection and focus between tabs. */
function onKeydown(event: KeyboardEvent) {
  const handled = ['ArrowRight', 'ArrowLeft', 'Home', 'End'];
  if (!handled.includes(event.key)) return;
  event.preventDefault();
  const options = props.options;
  const index = options.findIndex((option) => option.value === model.value);
  let next: number;
  if (event.key === 'ArrowRight') next = (index + 1) % options.length;
  else if (event.key === 'ArrowLeft') next = (index - 1 + options.length) % options.length;
  else if (event.key === 'Home') next = 0;
  else next = options.length - 1;
  const target = options[next];
  if (!target) return;
  model.value = target.value;
  const buttons = (event.currentTarget as HTMLElement).querySelectorAll('button');
  buttons[next]?.focus();
}
</script>

<template>
  <div
    role="tablist"
    :aria-label="ariaLabel"
    class="flex gap-0.5 rounded-sm border border-border bg-surface p-0.5"
    @keydown="onKeydown"
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
