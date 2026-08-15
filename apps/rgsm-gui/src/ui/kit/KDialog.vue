<script setup lang="ts">
import {
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogOverlay,
  DialogPortal,
  DialogRoot,
  DialogTitle,
} from 'reka-ui';
import { X } from '@lucide/vue';
import { $t } from '../../i18n';
import { LAYER } from '../layers';

const props = withDefaults(
  defineProps<{
    title?: string;
    /** Max width in px. */
    width?: number;
    /** When false: no X, no outside-click/Escape dismissal. For blocking confirms. */
    dismissable?: boolean;
  }>(),
  { title: undefined, width: 520, dismissable: true }
);

const open = defineModel<boolean>('open', { required: true });

function onInteractOutside(event: Event) {
  if (!props.dismissable) event.preventDefault();
}
function onEscape(event: KeyboardEvent) {
  if (!props.dismissable) event.preventDefault();
}
</script>

<template>
  <DialogRoot v-model:open="open">
    <DialogPortal>
      <DialogOverlay
        :style="{ zIndex: LAYER.dialog }"
        class="k-overlay fixed inset-0 bg-black/55 backdrop-blur-[2px]"
      />
      <DialogContent
        :style="{ zIndex: LAYER.dialog, maxWidth: `${width}px` }"
        class="k-dialog fixed left-1/2 top-1/2 w-[calc(100vw-2rem)] -translate-x-1/2 -translate-y-1/2 rounded-md border border-border bg-surface p-5 text-text shadow-overlay focus:outline-none"
        @interact-outside="onInteractOutside"
        @escape-key-down="onEscape"
      >
        <div v-if="title || !dismissable" class="mb-3 flex items-start justify-between gap-4">
          <DialogTitle v-if="title" class="text-base font-semibold leading-6">
            {{ title }}
          </DialogTitle>
          <DialogClose v-if="dismissable" as-child>
            <button
              type="button"
              :aria-label="$t('common.close')"
              class="ml-auto inline-flex h-7 w-7 cursor-pointer items-center justify-center rounded-sm border border-transparent bg-transparent text-text-dim transition-colors hover:bg-surface-2 hover:text-text focus-visible:outline-2 focus-visible:outline-accent"
            >
              <X :size="15" aria-hidden="true" />
            </button>
          </DialogClose>
        </div>
        <DialogDescription
          v-if="$slots.description"
          class="mb-4 text-sm leading-relaxed text-text-dim"
        >
          <slot name="description" />
        </DialogDescription>
        <slot />
        <div v-if="$slots.footer" class="mt-5 flex justify-end gap-2">
          <slot name="footer" />
        </div>
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>

<style>
@keyframes k-dialog-in {
  from {
    opacity: 0;
    transform: translate(-50%, -48%) scale(0.98);
  }
}
@keyframes k-overlay-in {
  from {
    opacity: 0;
  }
}
.k-dialog[data-state='open'] {
  animation: k-dialog-in 150ms ease-out;
}
.k-overlay[data-state='open'] {
  animation: k-overlay-in 150ms ease-out;
}
.k-dialog[data-state='closed'],
.k-overlay[data-state='closed'] {
  transition: opacity 120ms ease-in;
  opacity: 0;
}
</style>
