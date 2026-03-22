import { computed, type Component } from 'vue';
import {
  DocumentAdd,
  HotWater,
  InfoFilled,
  MostlyCloudy,
  Setting,
  SwitchFilled,
} from '@element-plus/icons-vue';
import { $t } from '../i18n';

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
    { text: $t('sidebar.homepage'), link: '/', icon: HotWater },
    { text: $t('sidebar.add_game'), link: '/AddGame', icon: DocumentAdd },
    { text: $t('sidebar.sync_settings'), link: '/SyncSettings', icon: MostlyCloudy },
    { text: $t('sidebar.settings'), link: '/Settings', icon: Setting },
    { text: $t('sidebar.about'), link: '/About', icon: InfoFilled },
  ]);

  const linksWithGames = computed<NavigationLink[]>(() => {
    const list = [...baseLinks.value];
    config.value?.games.forEach((game) => {
      list.push({ text: game.name, link: `/Management/${game.name}`, icon: SwitchFilled });
    });
    return list;
  });

  return { baseLinks, linksWithGames };
}
