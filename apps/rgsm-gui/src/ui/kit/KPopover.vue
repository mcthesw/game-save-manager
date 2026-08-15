<script setup lang="ts">
import { PopoverContent, PopoverPortal, PopoverRoot, PopoverTrigger } from 'reka-ui';
import { LAYER } from '../layers';

withDefaults(
  defineProps<{
    side?: 'top' | 'bottom' | 'left' | 'right';
    align?: 'start' | 'center' | 'end';
    width?: number;
  }>(),
  { side: 'bottom', align: 'center', width: undefined }
);
</script>

<template>
  <PopoverRoot>
    <PopoverTrigger as-child>
      <slot />
    </PopoverTrigger>
    <PopoverPortal>
      <PopoverContent
        :side="side"
        :side-offset="8"
        :align="align"
        :style="{ zIndex: LAYER.kitPopover, width: width ? `${width}px` : undefined }"
        class="rounded-md border border-border bg-surface p-2 shadow-overlay"
      >
        <slot name="content" />
      </PopoverContent>
    </PopoverPortal>
  </PopoverRoot>
</template>
