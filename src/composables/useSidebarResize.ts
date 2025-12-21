import { ref, inject, onUnmounted, type Ref } from 'vue';

export interface SidebarResizeOptions {
  minWidth?: number;
  maxWidth?: number;
  initialWidth?: number;
}

/**
 * 提供侧边栏拖动调整大小功能的 composable
 * - sidebarWidth: 当前侧边栏宽度
 * - isResizing: 是否正在调整大小
 * - startResize: 开始调整大小的事件处理函数
 */
export function useSidebarResize(options?: SidebarResizeOptions) {
  const minWidth = options?.minWidth ?? 200;
  const maxWidth = options?.maxWidth ?? 400;
  const defaultWidth = options?.initialWidth ?? 240;

  // 从父组件注入侧边栏宽度，如果没有则使用默认值
  const sidebarWidth = inject<Ref<number>>('sidebarWidth', ref(defaultWidth));
  const isResizing = ref(false);
  const startX = ref(0);
  const startWidth = ref(0);

  function handleMouseMove(event: MouseEvent) {
    if (!isResizing.value) return;
    const delta = event.clientX - startX.value;
    const newWidth = Math.max(minWidth, Math.min(maxWidth, startWidth.value + delta));
    sidebarWidth.value = newWidth;
  }

  function stopResize() {
    isResizing.value = false;
    document.removeEventListener('mousemove', handleMouseMove);
    document.removeEventListener('mouseup', stopResize);
  }

  function startResize(event: MouseEvent) {
    event.preventDefault();
    isResizing.value = true;
    startX.value = event.clientX;
    startWidth.value = sidebarWidth.value;
    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', stopResize);
  }

  // 组件卸载时清理事件监听器
  onUnmounted(() => {
    document.removeEventListener('mousemove', handleMouseMove);
    document.removeEventListener('mouseup', stopResize);
  });

  return { sidebarWidth, isResizing, startResize };
}
