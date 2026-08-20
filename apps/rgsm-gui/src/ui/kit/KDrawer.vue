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
import { computed } from 'vue';
import { X } from '@lucide/vue';
import { $t } from '../../i18n';
import { LAYER } from '../layers';
import { useOverlayDepth } from '../overlayDepth';

const props = withDefaults(
  defineProps<{
    title?: string;
    /** Panel width — px number or any CSS length (e.g. '70%'). */
    width?: number | string;
    /** When false: no X, no outside-click/Escape dismissal. */
    dismissable?: boolean;
  }>(),
  { title: undefined, width: 640, dismissable: true }
);

const open = defineModel<boolean>('open', { required: true });
useOverlayDepth(open);

const widthStyle = computed(() =>
  typeof props.width === 'number' ? `${props.width}px` : props.width
);

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
        :style="{ zIndex: LAYER.drawer }"
        class="k-overlay fixed inset-0 bg-black/55 backdrop-blur-[2px]"
      />
      <DialogContent
        :style="{ zIndex: LAYER.drawer, width: widthStyle }"
        class="k-drawer fixed right-0 top-0 flex h-full max-w-[calc(100vw-2rem)] flex-col border-l border-border bg-surface text-text shadow-overlay focus:outline-none"
        @interact-outside="onInteractOutside"
        @escape-key-down="onEscape"
      >
        <div class="flex shrink-0 items-center gap-3 border-b border-border px-5 py-3.5">
          <DialogTitle class="mr-auto text-base font-semibold leading-6">
            <slot name="title">{{ title }}</slot>
          </DialogTitle>
          <DialogClose v-if="dismissable" as-child>
            <button
              type="button"
              :aria-label="$t('common.close')"
              class="inline-flex h-7 w-7 cursor-pointer items-center justify-center rounded-sm border border-transparent bg-transparent text-text-dim transition-colors hover:bg-surface-2 hover:text-text focus-visible:outline-2 focus-visible:outline-accent"
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
