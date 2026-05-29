import type { Config, Game, GameAutomationSettings } from '../bindings';

/**
 * Whether a stored automation entry refers to the given game. Prefers the stable
 * `storage_key` when both sides have one, falling back to the display name.
 */
export function automationMatchesGame(automation: GameAutomationSettings, game: Game): boolean {
  if (automation.storage_key && game.storage_key) {
    return automation.storage_key === game.storage_key;
  }
  return automation.game_name === game.name;
}

/** Find the automation settings stored for a game, if any. */
export function findGameAutomation(config: Config, game: Game): GameAutomationSettings | undefined {
  return config.quick_action?.game_automations?.find((automation) =>
    automationMatchesGame(automation, game)
  );
}

/**
 * Whether the game has any auto-save behaviour configured: timer auto-backup,
 * or a process trigger.
 */
export function isAutoSaveConfigured(config: Config, game: Game): boolean {
  const automation = findGameAutomation(config, game);
  const hasProcessTrigger = Boolean(
    automation?.on_process_start ||
    automation?.on_process_exit ||
    automation?.in_process_interval_secs != null
  );
  return Boolean(game.auto_backup || hasProcessTrigger);
}
