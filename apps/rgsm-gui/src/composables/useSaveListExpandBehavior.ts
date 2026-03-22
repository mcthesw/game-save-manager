import { computed, watch, nextTick, onMounted, type Ref } from 'vue';
import type { MenuInstance } from 'element-plus';

export interface SaveListExpandOptions {
  menuRef: Ref<MenuInstance | undefined>;
  saveListMenuIndex?: string;
  filteredGames?: Ref<unknown[]>;
  showFavorite?: Ref<boolean>;
  searchQuery?: Ref<string>;
}

/**
 * 管理侧边栏保存列表展开/折叠行为的 composable
 * - saveListDefaultOpeneds: 默认展开的菜单项索引
 * - handleMenuOpen: 菜单展开事件处理函数
 * - handleMenuClose: 菜单折叠事件处理函数
 */
export function useSaveListExpandBehavior(options: SaveListExpandOptions) {
  const { config, saveConfig } = useConfig();
  const { menuRef, filteredGames, showFavorite, searchQuery } = options;
  const saveListMenuIndex = options.saveListMenuIndex ?? 'save-list';

  function isSearchActive() {
    return !!searchQuery?.value.trim();
  }

  function getSaveListBehavior() {
    return config.value.settings?.save_list_expand_behavior ?? 'always_closed';
  }

  function getSavedExpandState() {
    return config.value.settings?.save_list_last_expanded ?? false;
  }

  function shouldExpandSaveList() {
    // Always expand when searching so results are visible
    if (isSearchActive()) return true;

    const behavior = getSaveListBehavior();
    if (behavior === 'always_open') {
      return true;
    }
    if (behavior === 'remember_last') {
      return getSavedExpandState();
    }
    return false;
  }

  const saveListDefaultOpeneds = computed(() =>
    shouldExpandSaveList() ? [saveListMenuIndex] : []
  );

  async function applySaveListExpandState() {
    await nextTick();
    const menu = menuRef.value;
    if (!menu) {
      return;
    }
    if (shouldExpandSaveList()) {
      menu.open(saveListMenuIndex);
    } else {
      menu.close(saveListMenuIndex);
    }
  }

  async function persistSaveListState(expanded: boolean) {
    if (getSavedExpandState() === expanded) {
      return;
    }
    if (!config.value.settings) {
      return;
    }
    config.value.settings.save_list_last_expanded = expanded;
    await saveConfig();
  }

  async function handleMenuOpen(index: string) {
    if (index !== saveListMenuIndex) {
      return;
    }
    if (getSaveListBehavior() === 'remember_last') {
      await persistSaveListState(true);
    }
  }

  async function handleMenuClose(index: string) {
    if (index !== saveListMenuIndex) {
      return;
    }
    if (getSaveListBehavior() === 'remember_last') {
      await persistSaveListState(false);
    }
  }

  // 监听设置变化
  watch(
    () => config.value.settings?.save_list_expand_behavior,
    async (behavior) => {
      await applySaveListExpandState();
      if (behavior === 'always_open') {
        await persistSaveListState(true);
      } else if (behavior === 'always_closed') {
        await persistSaveListState(false);
      }
    },
    { immediate: true }
  );

  watch(
    () => config.value.settings?.save_list_last_expanded,
    async () => {
      if (getSaveListBehavior() === 'remember_last') {
        await applySaveListExpandState();
      }
    }
  );

  // 监听过滤后的游戏列表变化
  if (filteredGames) {
    watch(
      filteredGames,
      () => {
        void applySaveListExpandState();
      },
      { deep: true }
    );
  }

  // 监听收藏视图切换
  if (showFavorite) {
    watch(showFavorite, (value) => {
      if (!value) {
        void applySaveListExpandState();
      }
    });
  }

  // Auto-expand when search is active, restore configured state when cleared
  if (searchQuery) {
    watch(searchQuery, async (query) => {
      if (query.trim()) {
        await nextTick();
        menuRef.value?.open(saveListMenuIndex);
      } else {
        await applySaveListExpandState();
      }
    });
  }

  onMounted(() => {
    void applySaveListExpandState();
  });

  return { saveListDefaultOpeneds, handleMenuOpen, handleMenuClose };
}
