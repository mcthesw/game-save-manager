import type { QuickActionCompleted } from '../../api/commands';

/**
 * Whether a quick action changed (or may have changed) the snapshots shown for
 * a game. Games are matched by storage key because display names can repeat.
 * Failures count: a half-finished restore or backup leaves files behind.
 */
export function quickActionAffectsGame(
  event: QuickActionCompleted,
  game: { storage_key?: string }
): boolean {
  if (!event.game_id || event.game_id !== game.storage_key) return false;
  return event.status !== 'SkippedUnchanged';
}
