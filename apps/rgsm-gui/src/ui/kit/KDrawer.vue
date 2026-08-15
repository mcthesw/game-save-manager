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
    /** Panel width in px. */
    width?: number;
    /** When false: no X, no outside-click/Escape dismissal. */
    dismissable?: boolean;
  }>(),
  { title: undefined, width: 640, dismissable: true }
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
        :style="{ zIndex: LAYER.dialog, width: `${width}px` }"
        class="k-drawer fixed right-0 top-0 flex h-full max-w-[calc(100vw-2rem)] flex-col border-l border-border bg-surface text-text shadow-overlay focus:outline-none"
        @interact-outside="onInteractOutside"
        @escape-key-down="onEscape"
      >
        <div
          class="flex shrink-0 items-center justify-between gap-4 border-b border-border px-5 py-3.5"
        >
          <DialogTitle class="text-base font-semibold leading-6">
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
        <DialogDescription v-if="$slots.description" class="sr-only">
          <slot name="description" />
        </DialogDescription>
        <div class="min-h-0 flex-1 overflow-x-hidden overflow-y-auto px-5 py-4">
          <slot />
        </div>
        <div
          v-if="$slots.footer"
          class="flex shrink-0 items-center justify-end gap-2 border-t border-border px-5 py-3"
        >
          <slot name="footer" />
        </div>
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>

<style>
@keyframes k-drawer-in {
  from {
    transform: translateX(24px);
    opacity: 0;
  }
}
.k-drawer[data-state='open'] {
  animation: k-drawer-in 180ms cubic-bezier(0.4, 0, 0.2, 1);
}
.k-drawer[data-state='closed'] {
  transition:
    transform 140ms ease-in,
    opacity 140ms ease-in;
  transform: translateX(24px);
  opacity: 0;
}
</style>
