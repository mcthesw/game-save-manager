import { computed, type Component } from 'vue';
import { Cloud, Gamepad2, Home, Info, Settings } from '@lucide/vue';
import { $t } from '../i18n';
import { getGameManagementPath } from './useGameManagementRoute';

export interface NavigationLink {
  text: string;
  link: string;
  icon: Component;
}

const { config } = useConfig();

/**
 * 提供导航链接的 composable
 * - baseLinks: 基础导航链接（静态页面）
 * - linksWithGames: 基础链接 + 游戏管理页面链接
 */
export function useNavigationLinks() {
  const baseLinks = computed<NavigationLink[]>(() => [
    { text: $t('sidebar.homepage'), link: '/', icon: Home },
    { text: $t('sidebar.sync_settings'), link: '/SyncSettings', icon: Cloud },
    { text: $t('sidebar.settings'), link: '/Settings', icon: Settings },
    { text: $t('sidebar.about'), link: '/About', icon: Info },
  ]);

  const linksWithGames = computed<NavigationLink[]>(() => {
    const list = [...baseLinks.value];
    config.value?.games.forEach((game) => {
      list.push({ text: game.name, link: getGameManagementPath(game.name), icon: Gamepad2 });
    });
    return list;
  });

  return { baseLinks, linksWithGames };
}
