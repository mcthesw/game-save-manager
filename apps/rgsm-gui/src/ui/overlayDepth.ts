import { watch, type Ref } from 'vue';
import { ref } from 'vue';

/**
 * Count of currently open kit overlays (KDrawer/KDialog). Floating shell UI
 * (activity FAB) yields to overlays so it never covers their footer actions.
 */
export const overlayDepth = ref(0);

/** Register an overlay's open state; balanced via watch cleanup on close/unmount. */
export function useOverlayDepth(open: Ref<boolean>) {
  watch(
    open,
    (visible, _prev, onCleanup) => {
      if (!visible) return;
      overlayDepth.value += 1;
      onCleanup(() => {
        overlayDepth.value = Math.max(0, overlayDepth.value - 1);
      });
    },
    { immediate: true }
  );
}
