<script setup lang="ts">
import { computed, useSlots } from 'vue';
import { LoaderCircle } from '@lucide/vue';

/**
 * Kit button. Accent (primary) is reserved for main actions — one per view.
 * Icon-only buttons are legal only with an explicit `ariaLabel`.
 */
type Props = {
  variant?: 'primary' | 'default' | 'danger' | 'ghost';
  size?: 'sm' | 'md';
  loading?: boolean;
  disabled?: boolean;
  type?: 'button' | 'submit';
  ariaLabel?: string;
};

const props = withDefaults(defineProps<Props>(), {
  variant: 'default',
  size: 'md',
  loading: false,
  disabled: false,
  type: 'button',
  ariaLabel: undefined,
});

const slots = useSlots();
const hasText = computed(() =>
  Boolean(
    slots.default?.().some((node) => {
      // Whitespace-only text nodes don't count as a label.
      return !(typeof node.children === 'string' && node.children.trim() === '');
    })
  )
);

if (import.meta.env.DEV && !hasText.value && !props.ariaLabel) {
  console.warn('[kit] KButton: icon-only buttons must set ariaLabel.');
}

const classes = computed(() => {
  const base = [
    'inline-flex shrink-0 cursor-pointer select-none items-center justify-center gap-1.5',
    'rounded-sm font-medium transition-colors duration-150',
    'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent',
    'disabled:cursor-not-allowed disabled:opacity-50',
  ];
  const variants = {
    primary: 'bg-accent text-accent-contrast hover:brightness-110 active:brightness-95',
    default: 'border border-border bg-surface text-text hover:bg-surface-2',
    danger: 'bg-danger text-white hover:brightness-110 active:brightness-95',
    ghost: 'text-text hover:bg-surface-2',
  } as const;
  const sizes = {
    sm: hasText.value ? 'h-7 px-2.5 text-xs' : 'h-7 w-7 text-xs',
    md: hasText.value ? 'h-9 px-3.5 text-sm' : 'h-9 w-9 text-sm',
  } as const;
  return [...base, variants[props.variant], sizes[props.size]];
});
</script>

<template>
  <button
    :type="type"
    :class="classes"
    :disabled="disabled || loading"
    :aria-label="ariaLabel"
    :aria-busy="loading || undefined"
  >
    <LoaderCircle v-if="loading" :size="14" class="animate-spin" aria-hidden="true" />
    <slot v-else name="icon" />
    <slot />
  </button>
</template>
