import { warn } from './utils/logger';
import { createRouter, createWebHistory } from 'vue-router';
import { routes, handleHotUpdate } from 'vue-router/auto-routes';
import { useConfig } from './composables/useConfig';
import {
  isValidAppDestination,
  managementGameExists,
  mapLegacyHomePage,
  getGameNameFromRouteParam,
} from './utils/appRoutes';

export const router = createRouter({
  history: createWebHistory(),
  routes,
});

if (import.meta.hot) {
  handleHotUpdate(router);
}

router.beforeEach(async (to) => {
  const { config, whenConfigReady } = useConfig();
  await whenConfigReady();

  if (to.path.startsWith('/Management')) {
    const routeParam = 'name' in to.params ? to.params.name : undefined;
    if (managementGameExists(config.value.games, routeParam)) {
      return true;
    }
    void warn(`Game ${getGameNameFromRouteParam(routeParam)} not found`);
    return '/';
  }

  const mapped = mapLegacyHomePage(to.path);
  if (mapped !== to.path) {
    return mapped;
  }
  if (isValidAppDestination(to.path, config.value.games)) {
    return true;
  }

  void warn(`Page ${to.fullPath} not found`);
  return '/';
});

export function navigateTo(path: string) {
  return router.push(path);
}
