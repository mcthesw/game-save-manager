<script setup lang="ts">
import type { Component } from 'vue';
import { CheckCircle2, Info, TriangleAlert, X, XCircle } from '@lucide/vue';
import { $t } from '../../i18n';
import { LAYER } from '../layers';
import { dismissToast, useToast, type ToastTone } from '../../composables/useToast';

const { toasts } = useToast();

const toneIcon: Record<ToastTone, Component> = {
  success: CheckCircle2,
  info: Info,
  warning: TriangleAlert,
  error: XCircle,
};

const toneColor: Record<ToastTone, string> = {
  success: 'var(--success)',
  info: 'var(--text-dim)',
  warning: 'var(--warning)',
  error: 'var(--danger)',
};
</script>

<template>
  <div
    class="pointer-events-none fixed right-5 top-5 flex w-[340px] flex-col gap-3"
    :style="{ zIndex: LAYER.toast }"
    aria-live="polite"
  >
    <TransitionGroup name="k-toast">
      <div
        v-for="toast in toasts"
        :key="toast.id"
        class="k-toast-item pointer-events-auto relative flex gap-3 rounded-md border border-border bg-surface p-3.5 pr-9 shadow-overlay"
      >
        <component
          :is="toneIcon[toast.tone]"
          :size="20"
          class="mt-px shrink-0"
          :style="{ color: toneColor[toast.tone] }"
          aria-hidden="true"
        />
        <div class="min-w-0 flex-1">
          <div class="text-sm font-semibold leading-5 text-text">{{ toast.title }}</div>
          <div
            v-if="toast.description"
            class="mt-1 break-words text-[13px] leading-relaxed text-text-dim"
          >
            {{ toast.description }}
          </div>
        </div>
        <button
          type="button"
          :aria-label="$t('common.close')"
          class="absolute right-2.5 top-2.5 inline-flex h-5 w-5 cursor-pointer items-center justify-center rounded-sm border border-transparent bg-transparent text-text-dim transition-colors hover:bg-surface-2 hover:text-text focus-visible:outline-2 focus-visible:outline-accent"
          @click="dismissToast(toast.id)"
        >
          <X :size="13" aria-hidden="true" />
        </button>
      </div>
    </TransitionGroup>
  </div>
</template>

<style>
.k-toast-enter-active,
.k-toast-leave-active {
  transition:
    opacity 180ms ease,
    transform 180ms ease;
}
.k-toast-enter-from,
.k-toast-leave-to {
  opacity: 0;
  transform: translateX(24px);
}
.k-toast-move {
  transition: transform 180ms ease;
}
</style>
