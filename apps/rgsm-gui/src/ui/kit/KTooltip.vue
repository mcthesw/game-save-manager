<script setup lang="ts">
import {
  TooltipContent,
  TooltipPortal,
  TooltipProvider,
  TooltipRoot,
  TooltipTrigger,
} from 'reka-ui';
import { LAYER } from '../layers';

withDefaults(
  defineProps<{
    content: string;
    side?: 'top' | 'bottom' | 'left' | 'right';
  }>(),
  { side: 'top' }
);
</script>

<template>
  <TooltipProvider :delay-duration="250">
    <TooltipRoot>
      <TooltipTrigger as-child>
        <slot />
      </TooltipTrigger>
      <TooltipPortal>
        <TooltipContent
          :side="side"
          :side-offset="6"
          :style="{ zIndex: LAYER.kitPopover }"
          class="max-w-64 rounded-sm border border-border bg-surface px-2 py-1 text-xs leading-relaxed text-text shadow-overlay"
        >
          {{ content }}
        </TooltipContent>
      </TooltipPortal>
    </TooltipRoot>
  </TooltipProvider>
</template>
