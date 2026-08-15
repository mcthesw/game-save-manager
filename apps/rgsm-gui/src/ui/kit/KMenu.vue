<script setup lang="ts">
import type { Component } from 'vue';
import {
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuPortal,
  DropdownMenuRoot,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from 'reka-ui';
import { Check } from '@lucide/vue';
import { LAYER } from '../layers';

export type KMenuEntry =
  | {
      type: 'item';
      key: string;
      label: string;
      /** Second line of dim helper text — for option semantics, not decoration. */
      description?: string;
      icon?: Component;
      /** Renders a trailing check — for togglable entries reflecting current state. */
      active?: boolean;
      danger?: boolean;
      disabled?: boolean;
    }
  | { type: 'separator' };

defineProps<{
  entries: KMenuEntry[];
  ariaLabel?: string;
}>();

const emit = defineEmits<{
  select: [key: string];
}>();
</script>

<template>
  <DropdownMenuRoot>
    <DropdownMenuTrigger as-child :aria-label="ariaLabel">
      <slot />
    </DropdownMenuTrigger>
    <DropdownMenuPortal>
      <DropdownMenuContent
        align="end"
        :side-offset="6"
        :style="{ zIndex: LAYER.kitPopover }"
        class="min-w-44 rounded-md border border-border bg-surface p-1 shadow-overlay"
      >
        <template
          v-for="(entry, index) in entries"
          :key="entry.type === 'item' ? entry.key : index"
        >
          <DropdownMenuSeparator
            v-if="entry.type === 'separator'"
            class="mx-1 my-1 h-px bg-border"
          />
          <DropdownMenuItem
            v-else
            :disabled="entry.disabled"
            class="flex cursor-pointer select-none items-center gap-2 rounded-sm px-2 py-1.5 text-xs outline-none data-[disabled]:cursor-not-allowed data-[highlighted]:bg-surface-2 data-[disabled]:opacity-50"
            :class="[entry.danger ? 'text-danger' : 'text-text', { 'max-w-72': entry.description }]"
            @select="emit('select', entry.key)"
          >
            <component
              :is="entry.icon"
              v-if="entry.icon"
              :size="13"
              class="shrink-0"
              :class="{ 'mt-0.5 self-start': entry.description }"
              aria-hidden="true"
            />
            <span class="min-w-0 flex-1">
              <span class="block truncate">{{ entry.label }}</span>
              <span
                v-if="entry.description"
                class="mt-0.5 block text-[11px] leading-snug text-text-dim"
                >{{ entry.description }}</span
              >
            </span>
            <Check
              v-if="entry.active"
              :size="13"
              class="shrink-0 text-accent"
              :class="{ 'mt-0.5 self-start': entry.description }"
              aria-hidden="true"
            />
          </DropdownMenuItem>
        </template>
      </DropdownMenuContent>
    </DropdownMenuPortal>
  </DropdownMenuRoot>
</template>
