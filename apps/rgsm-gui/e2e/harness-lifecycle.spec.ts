import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { seedEmptyCloudWithLocalGame } from './support/cloud-fixture';
import { createRunRoot, hostPost, removeRunRoot, workspacePath } from './support/rgsm-instance';
import { startDualSession } from './support/session';
import { hasServerAddress, startTestWebServer } from './support/test-web';
import { VITE_ORIGIN } from './support/constants';
import { seedLocalConfig } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import type { Game } from '../src/api/generated/types.gen';
import { waitForCdpPage } from './support/cdp-page';

test('connected page lookup waits for page creation and navigation', async ({ browser }) => {
  const appUrl = `data:text/html,${encodeURIComponent('<title>App page</title>')}`;
  const context = await browser.newContext();
  const pending = waitForCdpPage(browser, appUrl);
  try {
    const page = await context.newPage();
    expect(page.url()).toBe('about:blank');
    await page.goto(appUrl);
    expect(await pending).toBe(page);
    await expect(page).toHaveTitle('App page');
  } finally {
    await context.close();
    await pending.catch(() => undefined);
  }
});

test('server readiness accepts a port wrapped in Vite color codes', () => {
  expect(hasServerAddress('Local: http://localhost:\u001b[1m5173\u001b[22m/', VITE_ORIGIN)).toBe(
    true
  );
  expect(hasServerAddress('VITE ready in 100 ms', VITE_ORIGIN)).toBe(false);
});

test('HTTP-only selection saves the quick backup game without a tray', async ({ browser }) => {
  const runRoot = await createRunRoot('quick-game-no-tray');
  const device = await seedLocalConfig(runRoot);
  const session = await startLocalSession(browser, {
    runRoot,
    device,
    label: 'quick-game-no-tray',
  });
  try {
    const before = await hostPost<{ games: Game[] }>(session.host, '/api/v1/get-local-config');
    expect(before.ok, before.raw).toBe(true);
    const game = before.data!.games[0]!;
    const selected = await hostPost(session.host, '/api/v1/set-quick-backup-game', { game });
    expect(selected.ok, selected.raw).toBe(true);
    const after = await hostPost<{ quick_action: { quick_action_game_id: string } }>(
      session.host,
      '/api/v1/get-local-config'
    );
    expect(after.data?.quick_action.quick_action_game_id).toBe(game.storage_key);
    await session.close(true);
    const log = await readFile(join(runRoot, 'logs', 'quick-game-no-tray.log'), 'utf8');
    expect(log).not.toContain('Cannot get tray');
    expect(log).not.toContain('Setting up tray icon');
    expect(log).not.toContain('Setting up hotkeys');
  } finally {
    await session.close();
  }
});

test('web setup rejects a busy port without adopting or killing its listener', async () => {
  let unexpected: Awaited<ReturnType<typeof startTestWebServer>> | undefined;
  try {
    await expect(async () => {
      unexpected = await startTestWebServer(workspacePath('apps', 'rgsm-gui'));
    }).rejects.toThrow(/Vite failed to start.*exited/s);
    expect((await fetch(VITE_ORIGIN)).ok).toBe(true);
  } finally {
    await unexpected?.stop();
  }
});

test('a failed second host startup closes the first host and retains diagnostic files', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('startup-cleanup');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  let session: Awaited<ReturnType<typeof startDualSession>> | undefined;
  try {
    await expect(async () => {
      session = await startDualSession(browser, {
        ...seeded,
        // The existing config file cannot be used as an application-data directory.
        deviceB: {
          ...seeded.deviceB,
          appDataDir: join(seeded.deviceB.appDataDir, 'GameSaveManager.config.json'),
        },
        runRoot,
        label: 'startup-cleanup',
      });
    }).rejects.toThrow();
    const host = JSON.parse(
      await readFile(join(seeded.deviceA.appDataDir, 'GameSaveManager.host.json'), 'utf8')
    );
    await expect(
      fetch(`http://127.0.0.1:${host.port}/api/v1/get-build-info`, {
        method: 'POST',
        signal: AbortSignal.timeout(1_000),
      })
    ).rejects.toThrow();
    const log = await readFile(join(runRoot, 'logs', 'startup-cleanup-a.log'), 'utf8');
    expect(log).not.toContain('Setting up tray icon');
    expect(log).not.toContain('Setting up hotkeys');
  } finally {
    await session?.close(true);
    await removeRunRoot(runRoot);
  }
});
