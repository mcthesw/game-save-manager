import { warn } from '@tauri-apps/plugin-log';
import { createRouter, createWebHistory } from 'vue-router';
import { routes, handleHotUpdate } from 'vue-router/auto-routes';
import { useConfig } from './composables/useConfig';

const knownPages = new Set(['/', '/About', '/AddGame', '/Settings', '/SyncSettings']);

export const router = createRouter({
  history: createWebHistory(),
  routes,
});

if (import.meta.hot) {
  handleHotUpdate(router);
}

router.beforeEach((to) => {
  const { config } = useConfig();

  if (to.path.startsWith('/Management')) {
    const routeParam = 'name' in to.params ? to.params.name : undefined;
    const routeName = Array.isArray(routeParam) ? routeParam[0] : routeParam;
    const gameName = typeof routeName === 'string' ? routeName : '';
    const exists = config.value.games.some((game) => game.name === gameName);
    if (!exists) {
      void warn(`Game ${gameName} not found`);
      return '/';
    }
    return true;
  }

  if (!knownPages.has(to.path)) {
    void warn(`Page ${to.fullPath} not found`);
    return '/';
  }

  return true;
});

export function navigateTo(path: string) {
  return router.push(path);
}
