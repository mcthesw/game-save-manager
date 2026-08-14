import { ref, watch } from 'vue';
import type { Game } from '../api/commands';

export type SaveListSortMode = 'saved_order' | 'last_played' | 'name';
export type SaveListSortDirection = 'asc' | 'desc';

const STORAGE_KEY = 'rgsm.saveListSort.v1';

const playedAtByGame = ref<Record<string, string>>({});

let initialized = false;

function gameSortIdentity(game: Pick<Game, 'name' | 'storage_key'>) {
  return game.storage_key?.trim() || game.name;
}

function isSortMode(value: unknown): value is SaveListSortMode {
  return value === 'saved_order' || value === 'last_played' || value === 'name';
}

function isSortDirection(value: unknown): value is SaveListSortDirection {
  return value === 'asc' || value === 'desc';
}

function readStoredState() {
  if (typeof window === 'undefined') return;

  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return;

    const parsed = JSON.parse(raw) as {
      mode?: unknown;
      direction?: unknown;
      playedAtByGame?: unknown;
    };

    if (parsed.playedAtByGame && typeof parsed.playedAtByGame === 'object') {
      playedAtByGame.value = parsed.playedAtByGame as Record<string, string>;
    }
  } catch {
    playedAtByGame.value = {};
  }
}

function persistState() {
  if (typeof window === 'undefined') return;

  try {
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        playedAtByGame: playedAtByGame.value,
      })
    );
  } catch {
    // Sorting still works for the current session when browser storage is unavailable.
  }
}

function ensureInitialized() {
  if (initialized) return;
  initialized = true;

  readStoredState();
  watch(playedAtByGame, persistState, { deep: true });
}

function getSortMode(): SaveListSortMode {
  const { config } = useConfig();
  const mode = config.value.settings?.save_list_sort_mode;
  return isSortMode(mode) ? mode : 'saved_order';
}

function getSortDirection(): SaveListSortDirection {
  const { config } = useConfig();
  const direction = config.value.settings?.save_list_sort_direction;
  return isSortDirection(direction) ? direction : 'desc';
}

function compareBySavedOrder(
  leftIndex: number,
  rightIndex: number,
  direction: SaveListSortDirection
) {
  return direction === 'asc' ? leftIndex - rightIndex : rightIndex - leftIndex;
}

function compareByLastPlayed(
  left: Game,
  right: Game,
  leftIndex: number,
  rightIndex: number,
  direction: SaveListSortDirection
) {
  const leftTime = Date.parse(playedAtByGame.value[gameSortIdentity(left)] ?? '');
  const rightTime = Date.parse(playedAtByGame.value[gameSortIdentity(right)] ?? '');
  const leftPlayed = Number.isFinite(leftTime);
  const rightPlayed = Number.isFinite(rightTime);

  if (!leftPlayed && !rightPlayed) {
    return compareBySavedOrder(leftIndex, rightIndex, 'asc');
  }
  if (!leftPlayed) return 1;
  if (!rightPlayed) return -1;

  const diff = leftTime - rightTime;
  if (diff === 0) {
    return compareBySavedOrder(leftIndex, rightIndex, 'asc');
  }

  return direction === 'asc' ? diff : -diff;
}

function compareByName(
  left: Game,
  right: Game,
  leftIndex: number,
  rightIndex: number,
  direction: SaveListSortDirection
) {
  const diff = left.name.localeCompare(right.name, undefined, {
    numeric: true,
    sensitivity: 'base',
  });

  if (diff === 0) {
    return compareBySavedOrder(leftIndex, rightIndex, 'asc');
  }

  return direction === 'asc' ? diff : -diff;
}

export function useSaveListSort() {
  ensureInitialized();

  function sortedGames(games: readonly Game[]) {
    const mode = getSortMode();
    const direction = getSortDirection();

    return games
      .map((game, index) => ({ game, index }))
      .sort((left, right) => {
        if (mode === 'last_played') {
          return compareByLastPlayed(left.game, right.game, left.index, right.index, direction);
        }
        if (mode === 'name') {
          return compareByName(left.game, right.game, left.index, right.index, direction);
        }
        return compareBySavedOrder(left.index, right.index, direction);
      })
      .map(({ game }) => game);
  }

  function markGamePlayed(game: Pick<Game, 'name' | 'storage_key'>, playedAt = new Date()) {
    const identity = gameSortIdentity(game);
    if (!identity) return;

    playedAtByGame.value = {
      ...playedAtByGame.value,
      [identity]: playedAt.toISOString(),
    };
  }

  return {
    sortedGames,
    markGamePlayed,
  };
}
