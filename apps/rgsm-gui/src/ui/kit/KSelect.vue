<script setup lang="ts">
import {
  SelectContent,
  SelectItem,
  SelectItemIndicator,
  SelectItemText,
  SelectPortal,
  SelectRoot,
  SelectScrollDownButton,
  SelectScrollUpButton,
  SelectTrigger,
  SelectValue,
  SelectViewport,
} from 'reka-ui';
import { Check, ChevronDown, X } from '@lucide/vue';
import { $t } from '../../i18n';
import { LAYER } from '../layers';

export type KSelectOption = {
  label: string;
  value: string | number;
  disabled?: boolean;
};

const props = withDefaults(
  defineProps<{
    options: KSelectOption[];
    placeholder?: string;
    clearable?: boolean;
    disabled?: boolean;
    size?: 'sm' | 'md';
    ariaLabel?: string;
  }>(),
  {
    placeholder: undefined,
    clearable: false,
    disabled: false,
    size: 'md',
    ariaLabel: undefined,
  }
);

const model = defineModel<string | number>();

function clear(event: Event) {
  event.stopPropagation();
  model.value = undefined;
}
</script>

<template>
  <SelectRoot v-model="model" :disabled="disabled">
    <SelectTrigger
      :aria-label="ariaLabel"
      class="relative inline-flex w-full cursor-pointer items-center justify-between gap-2 rounded-sm border border-border bg-surface px-3 text-left text-sm text-text transition-colors duration-150 focus:border-accent focus:outline-none disabled:cursor-not-allowed disabled:opacity-50 data-[placeholder]:text-text-dim"
      :class="size === 'sm' ? 'h-7 text-xs' : 'h-9'"
    >
      <SelectValue :placeholder="placeholder" />
      <span class="flex shrink-0 items-center gap-0.5">
        <button
          v-if="clearable && model !== undefined && !disabled"
          type="button"
          :aria-label="$t('common.close')"
          class="inline-flex h-5 w-5 cursor-pointer items-center justify-center rounded-sm text-text-dim hover:text-text"
          @click="clear"
          @pointerdown.stop
        >
          <X :size="12" aria-hidden="true" />
        </button>
        <ChevronDown :size="14" class="text-text-dim" aria-hidden="true" />
      </span>
    </SelectTrigger>
    <SelectPortal>
      <SelectContent
        position="popper"
        :side-offset="4"
        :style="{ zIndex: LAYER.kitPopover }"
        class="max-h-72 w-[var(--reka-select-trigger-width)] overflow-hidden rounded-sm border border-border bg-surface text-text shadow-overlay"
      >
        <SelectScrollUpButton class="flex items-center justify-center py-1 text-text-dim"
          >▲</SelectScrollUpButton
        >
        <SelectViewport class="p-1">
          <SelectItem
            v-for="option in props.options"
            :key="String(option.value)"
            :value="option.value"
            :disabled="option.disabled"
            class="flex h-8 cursor-pointer select-none items-center justify-between gap-2 rounded-sm px-2 text-sm outline-none data-[disabled]:cursor-not-allowed data-[highlighted]:bg-surface-2 data-[disabled]:opacity-50"
          >
            <SelectItemText>{{ option.label }}</SelectItemText>
            <SelectItemIndicator
              ><Check :size="14" class="text-accent" aria-hidden="true"
            /></SelectItemIndicator>
          </SelectItem>
        </SelectViewport>
        <SelectScrollDownButton class="flex items-center justify-center py-1 text-text-dim"
          >▼</SelectScrollDownButton
        >
      </SelectContent>
    </SelectPortal>
  </SelectRoot>
</template>
