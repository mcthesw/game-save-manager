<script setup lang="ts">
import { computed } from 'vue';
import { ChevronDown } from '@lucide/vue';
import { KCheckbox, KPopover } from '../ui/kit';

const props = withDefaults(
  defineProps<{
    options: { value: number; label: string }[];
    modelValue: number[];
    placeholder?: string;
    ariaLabel?: string;
  }>(),
  { placeholder: undefined, ariaLabel: undefined }
);

const emit = defineEmits<{
  (event: 'update:modelValue', ids: number[]): void;
}>();

const selectedLabels = computed(() =>
  props.options.filter((option) => props.modelValue.includes(option.value)).map((o) => o.label)
);

function toggle(id: number) {
  const next = props.modelValue.includes(id)
    ? props.modelValue.filter((existing) => existing !== id)
    : [...props.modelValue, id];
  emit('update:modelValue', next);
}
</script>

<template>
  <KPopover side="bottom" align="start" :width="320">
    <button
      type="button"
      :aria-label="ariaLabel ?? placeholder"
      class="flex h-9 w-full cursor-pointer items-center justify-between gap-2 rounded-sm border border-border bg-surface px-3 text-sm text-text transition-colors hover:border-border-strong focus-visible:outline-2 focus-visible:outline-accent"
    >
      <span
        class="min-w-0 flex-1 truncate text-left"
        :class="{ 'text-text-dim': selectedLabels.length === 0 }"
      >
        {{ selectedLabels.length > 0 ? selectedLabels.join(', ') : placeholder }}
      </span>
      <ChevronDown :size="14" class="shrink-0 text-text-dim" aria-hidden="true" />
    </button>
    <template #content>
      <div class="flex max-h-64 flex-col gap-0.5 overflow-y-auto">
        <div
          v-for="option in options"
          :key="option.value"
          class="flex cursor-pointer items-center gap-2 rounded-sm px-2 py-1.5 text-xs text-text transition-colors hover:bg-surface-2"
          @click="toggle(option.value)"
        >
          <KCheckbox
            :model-value="modelValue.includes(option.value)"
            :aria-label="option.label"
            class="pointer-events-none"
          />
          <span class="truncate">{{ option.label }}</span>
        </div>
      </div>
    </template>
  </KPopover>
</template>
