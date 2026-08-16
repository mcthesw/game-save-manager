import { getGameNameFromRouteParam } from '../utils/appRoutes';

export function getGameManagementPath(gameName: string): string {
  return `/Management/${encodeURIComponent(gameName)}`;
}

export { getGameNameFromRouteParam };
