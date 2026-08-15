<script setup lang="ts">
import { SwitchRoot, SwitchThumb } from 'reka-ui';

withDefaults(
  defineProps<{
    disabled?: boolean;
    ariaLabel?: string;
  }>(),
  { disabled: false, ariaLabel: undefined }
);

const checked = defineModel<boolean>({ default: false });
</script>

<template>
  <SwitchRoot
    v-model="checked"
    :disabled="disabled"
    :aria-label="ariaLabel"
    class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full border border-border bg-surface-2 transition-colors duration-150 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:border-accent data-[state=checked]:bg-accent"
  >
    <SwitchThumb
      class="block h-3.5 w-3.5 translate-x-0.5 rounded-full bg-text-dim transition-transform duration-150 data-[state=checked]:translate-x-[19px] data-[state=checked]:bg-accent-contrast"
    />
  </SwitchRoot>
</template>
