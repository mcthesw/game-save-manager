<script setup lang="ts">
import { ref } from 'vue';
import { X } from '@lucide/vue';

const props = withDefaults(
  defineProps<{
    placeholder?: string;
    disabled?: boolean;
    ariaLabel?: string;
  }>(),
  { placeholder: undefined, disabled: false, ariaLabel: undefined }
);

const tags = defineModel<string[]>({ required: true });

const draft = ref('');
const inputRef = ref<HTMLInputElement | null>(null);

function addDraft() {
  const value = draft.value.trim();
  if (!value) return;
  if (!tags.value.includes(value)) {
    tags.value = [...tags.value, value];
  }
  draft.value = '';
}

function removeAt(index: number) {
  tags.value = tags.value.filter((_, i) => i !== index);
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Enter' || event.key === ',') {
    event.preventDefault();
    addDraft();
    return;
  }
  if (event.key === 'Backspace' && draft.value === '' && tags.value.length > 0) {
    removeAt(tags.value.length - 1);
  }
}

function focusInput() {
  if (!props.disabled) inputRef.value?.focus();
}
</script>

<template>
  <div
    class="flex min-h-9 w-full cursor-text flex-wrap items-center gap-1.5 rounded-sm border border-border bg-surface px-2 py-1.5 transition-colors duration-150 focus-within:border-accent box-border"
    :class="{ 'cursor-not-allowed opacity-50': disabled }"
    @click="focusInput"
  >
    <span
      v-for="(tag, index) in tags"
      :key="tag"
      class="inline-flex items-center gap-1 rounded-sm bg-surface-2 px-1.5 py-0.5 text-xs leading-4 text-text"
    >
      {{ tag }}
      <button
        type="button"
        :disabled="disabled"
        :aria-label="`Remove ${tag}`"
        class="inline-flex h-3.5 w-3.5 cursor-pointer items-center justify-center rounded-sm border border-transparent bg-transparent text-text-dim transition-colors hover:text-text disabled:cursor-not-allowed"
        @click.stop="removeAt(index)"
      >
        <X :size="11" aria-hidden="true" />
      </button>
    </span>
    <input
      ref="inputRef"
      v-model="draft"
      type="text"
      :placeholder="tags.length === 0 ? placeholder : undefined"
      :disabled="disabled"
      :aria-label="ariaLabel ?? placeholder"
      class="min-w-20 flex-1 border-none bg-transparent py-0.5 text-sm text-text outline-none placeholder:text-text-dim disabled:cursor-not-allowed"
      @keydown="onKeydown"
      @blur="addDraft"
    />
  </div>
</template>
