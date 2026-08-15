import { ref } from 'vue';

/**
 * 全局「添加/编辑游戏」抽屉状态。模块级单例，任意入口（侧栏、主页状态格、
 * 管理页编辑按钮）都可打开；传入游戏名进入编辑模式，否则为新建模式。
 */
const visible = ref(false);
const editGameName = ref<string | null>(null);

function open(gameName?: string) {
  editGameName.value = gameName ?? null;
  visible.value = true;
}

function close() {
  visible.value = false;
}

export function useAddGameDrawer() {
  return { visible, editGameName, open, close };
}
