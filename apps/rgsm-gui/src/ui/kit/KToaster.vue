<script setup lang="ts">
import { X } from '@lucide/vue';
import { $t } from '../../i18n';
import { LAYER } from '../layers';
import { dismissToast, useToast, type ToastTone } from '../../composables/useToast';

const { toasts } = useToast();

const toneBar: Record<ToastTone, string> = {
  success: 'var(--success)',
  info: 'var(--text-dim)',
  warning: 'var(--warning)',
  error: 'var(--danger)',
};
</script>

<template>
  <div
    class="pointer-events-none fixed bottom-20 right-5 flex w-80 flex-col gap-2"
    :style="{ zIndex: LAYER.toast }"
    aria-live="polite"
  >
    <TransitionGroup name="k-toast">
      <div
        v-for="toast in toasts"
        :key="toast.id"
        class="k-toast-item pointer-events-auto flex gap-2.5 rounded-md border border-border bg-surface p-3 shadow-overlay"
      >
        <span
          class="w-0.5 shrink-0 self-stretch rounded-full"
          :style="{ background: toneBar[toast.tone] }"
          aria-hidden="true"
        />
        <div class="min-w-0 flex-1">
          <div class="text-sm font-medium leading-5 text-text">{{ toast.title }}</div>
          <div
            v-if="toast.description"
            class="mt-0.5 break-words text-xs leading-relaxed text-text-dim"
          >
            {{ toast.description }}
          </div>
        </div>
        <button
          type="button"
          :aria-label="$t('common.close')"
          class="inline-flex h-5 w-5 shrink-0 cursor-pointer items-center justify-center rounded-sm border border-transparent bg-transparent text-text-dim transition-colors hover:bg-surface-2 hover:text-text focus-visible:outline-2 focus-visible:outline-accent"
          @click="dismissToast(toast.id)"
        >
          <X :size="12" aria-hidden="true" />
        </button>
      </div>
    </TransitionGroup>
  </div>
</template>

<style>
.k-toast-enter-active,
.k-toast-leave-active {
  transition:
    opacity 150ms ease,
    transform 150ms ease;
}
.k-toast-enter-from,
.k-toast-leave-to {
  opacity: 0;
  transform: translateX(12px);
}
.k-toast-move {
  transition: transform 150ms ease;
}
</style>
