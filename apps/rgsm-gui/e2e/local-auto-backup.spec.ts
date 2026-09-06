import { test, expect } from '@playwright/test';
import { join } from 'node:path';
import { readFile } from 'node:fs/promises';
import type { Config, Game, QuickActionCompleted, Snapshot } from '../src/api/commands';
import { createRunRoot, hostPost, type RgsmHost } from './support/rgsm-instance';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';

async function setTimer(host: RgsmHost, gameId: string, interval: number | null) {
  const result = await hostPost(host, '/api/v1/set-game-auto-backup', {
    gameName: gameId,
    autoBackup: interval === null ? null : { interval_secs: interval, max_backup_count: null },
  });
  expect(result.ok, result.raw).toBe(true);
}

async function snapshots(host: RgsmHost, game: Game) {
  const result = await hostPost<{ backups: Snapshot[] }>(host, '/api/v1/get-game-snapshots-info', {
    game,
  });
  expect(result.ok, result.raw).toBe(true);
  return result.data.backups;
}

test('same-title games each create timer backups and keep independent indicators', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('independent-timers');
  const saves = [join(runRoot, 'first.sav'), join(runRoot, 'second.sav')];
  const device = await seedLocalConfig(runRoot, {
    games: ['first', 'second'].map((storageKey, index) => ({
      name: 'Same',
      storageKey,
      units: [{ type: 'File', path: saves[index] }],
    })),
  });
  for (const [index, path] of saves.entries()) await writeSaveText(path, `Game ${index}`);
  const session = await startLocalSession(browser, {
    runRoot,
    device,
    label: 'independent-timers',
  });
  let failed = false;
  try {
    const { host, page } = session;
    await setTimer(host, 'first', 1);
    await setTimer(host, 'second', 1);
    const status = await hostPost<Array<{ game_id: string; game_name: string }>>(
      host,
      '/api/v1/get-auto-backup-status'
    );
    expect(status.data.map((row) => row.game_id).sort()).toEqual(['first', 'second']);
    const config = await hostPost<Config>(host, '/api/v1/get-local-config');
    for (const game of config.data.games) {
      await expect
        .poll(
          async () =>
            (await snapshots(host, game)).filter((snapshot) => snapshot.created_by === 'Timer')
              .length,
          { timeout: 15_000 }
        )
        .toBe(1);
    }
    await setTimer(host, 'first', null);
    await page.reload();
    const rows = page.locator('.all-list .game-row');
    await expect(rows).toHaveCount(2);
    await expect(rows.nth(0).locator('.game-dot')).not.toHaveClass(/\bon\b/);
    await expect(rows.nth(1).locator('.game-dot')).toHaveClass(/\bon\b/);
    for (const [index, path] of saves.entries())
      expect(await readFile(path, 'utf8')).toBe(`Game ${index}`);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});

test('a real timer completion preserves unsaved settings without submitting them', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('timer-settings-draft');
  const device = await seedLocalConfig(runRoot);
  await writeSaveText(device.savePath, 'Timer source');
  const session = await startLocalSession(browser, {
    runRoot,
    device,
    label: 'timer-settings-draft',
  });
  let failed = false;
  try {
    const { page, host } = session;
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    await page.getByRole('button', { name: 'Quick Actions', exact: true }).click();
    const save = page.getByRole('button', { name: 'Save hotkeys', exact: true });
    await expect(save).toBeDisabled();
    const sound = page
      .getByText('Play sound when quick actions finish', { exact: true })
      .locator('..')
      .getByRole('switch');
    await expect(sound).not.toBeChecked();
    await sound.click();
    await expect(save).toBeEnabled();
    let writes = 0;
    page.on('request', (request) => {
      if (new URL(request.url()).pathname === '/api/v1/set-config') writes += 1;
    });
    await page.evaluate(async () => {
      const state = window as typeof window & { timerCompletions: QuickActionCompleted[] };
      state.timerCompletions = [];
      const modulePath = '/src/api/commands.ts';
      const { events } = await import(modulePath);
      await events.quickActionCompleted.listen((event: { payload: QuickActionCompleted }) => {
        if (event.payload.trigger === 'Timer' && event.payload.status === 'Success') {
          state.timerCompletions.push(event.payload);
        }
      });
    });
    const config = await hostPost<Config>(host, '/api/v1/get-local-config');
    const game = config.data.games[0];
    await setTimer(host, game.storage_key!, 1);
    await expect
      .poll(
        () =>
          page.evaluate(
            () =>
              (window as typeof window & { timerCompletions: QuickActionCompleted[] })
                .timerCompletions.length
          ),
        { timeout: 15_000 }
      )
      .toBeGreaterThan(0);
    await setTimer(host, game.storage_key!, null);
    expect((await snapshots(host, game)).some((snapshot) => snapshot.created_by === 'Timer')).toBe(
      true
    );
    // Give an erroneous config replacement time to trigger Settings' 500 ms autosave.
    await page.waitForTimeout(750);
    await expect(sound).toBeChecked();
    await expect(save).toBeEnabled();
    expect(writes).toBe(0);
    const persisted = await hostPost<Config>(host, '/api/v1/get-local-config');
    expect(persisted.data.quick_action?.enable_sound).toBe(false);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
