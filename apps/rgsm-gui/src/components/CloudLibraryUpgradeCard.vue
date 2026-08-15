<script setup lang="ts">
import { KButton } from '../ui/kit';

defineProps<{
  kicker: string;
  title: string;
  hint: string;
  action: string;
}>();

const emit = defineEmits<{
  (event: 'action'): void;
}>();
</script>

<template>
  <section
    class="mb-5 flex flex-wrap items-center justify-between gap-4 rounded-md border border-[color-mix(in_oklab,var(--warning)_38%,transparent)] bg-[color-mix(in_oklab,var(--warning)_10%,transparent)] px-5 py-4"
  >
    <div class="min-w-0">
      <p class="mb-1 text-xs font-bold tracking-wide text-warning">{{ kicker }}</p>
      <p class="text-base font-semibold text-text">{{ title }}</p>
      <p class="mt-1.5 text-[13px] leading-relaxed text-text-dim">{{ hint }}</p>
    </div>
    <KButton variant="primary" class="shrink-0" @click="emit('action')">
      {{ action }}
    </KButton>
  </section>
</template>
