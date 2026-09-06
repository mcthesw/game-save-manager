import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { localArchivePath } from './support/cloud-assertions';
import { readSave, seedEmptyCloudWithLocalGame, writeSave } from './support/cloud-fixture';
import {
  connectLibrary,
  createLibrary,
  createPublishedSnapshot,
  enableMode,
  openApp,
  openGame,
} from './support/gui';
import { createRunRoot, hostPost } from './support/rgsm-instance';
import { startDualSession } from './support/session';
import { createSnapshotForGame } from './support/local-gui';

test('many pending games keep the last comparison and Later accessible in a small window', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('progress-list');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const names = Array.from({ length: 14 }, (_, index) => `Pending game ${index + 1}`);
  for (const device of [seeded.deviceA, seeded.deviceB]) {
    const configPath = join(device.appDataDir, 'GameSaveManager.config.json');
    const config = JSON.parse(await readFile(configPath, 'utf8'));
    const template = config.games[0];
    config.games = names.map((name) => ({ ...template, name, storage_key: name }));
    await writeFile(configPath, JSON.stringify(config));
  }
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'progress-list' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    for (const name of names) {
      await createSnapshotForGame(session.hostA, name, 'Remote progress');
    }
    await connectLibrary(session.pageB);
    for (const name of names) {
      const result = await hostPost(session.hostB, '/api/v1/set-game-sync-mode', {
        gameId: name,
        mode: 'multi_device_sync',
        initialCatchUp: 'keep_remote',
        liveSave: null,
        enabled: true,
      });
      expect(result.ok, result.raw).toBe(true);
    }
    await session.pageB.setViewportSize({ width: 800, height: 500 });
    await session.pageB.bringToFront();
    await session.pageB.evaluate(async () => {
      const modulePath = '/src/composables/useCloudLibrary.ts';
      await (await import(modulePath)).refreshCloudLibrary(true);
    });
    const prompt = session.pageB.getByRole('dialog', {
      name: 'Progress available from other devices',
    });
    const comparisons = prompt.getByRole('button', { name: 'Compare', exact: true });
    await expect(comparisons).toHaveCount(names.length);
    const later = prompt.getByRole('button', { name: 'Later', exact: true });
    await expect(later).toBeInViewport();
    await comparisons.last().scrollIntoViewIfNeeded();
    await expect(comparisons.last()).toBeInViewport();
    await expect(later).toBeInViewport();
    await later.click();
    await expect(prompt).toBeHidden();
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});

test('remote progress prompts can be deferred across window reloads and never apply by themselves', async ({
  browser,
}, testInfo) => {
  const runRoot = await createRunRoot('progress-prompts');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, {
    ...seeded,
    runRoot,
    label: 'progress-prompts',
  });
  let failed = false;
  const refreshB = async () => {
    await session.pageB.bringToFront();
    await session.pageB.evaluate(async () => {
      const modulePath = '/src/composables/useCloudLibrary.ts';
      await (await import(modulePath)).refreshCloudLibrary();
    });
  };
  try {
    await createLibrary(session.pageA);
    const first = await createPublishedSnapshot(session.pageA, session.hostA, 'First on A');
    await connectLibrary(session.pageB);
    const before = await readSave(seeded.deviceB);
    await enableMode(session.pageB, session.hostB, 'Multi-device Sync', 'Keep in cloud');
    await refreshB();
    const prompt = session.pageB.getByRole('dialog', {
      name: 'Progress available from other devices',
    });
    await expect(prompt).toBeVisible({ timeout: 10_000 });
    await session.pageB.reload();
    await expect(prompt).toBeVisible();
    await testInfo.attach('remote-progress-prompt', {
      body: await session.pageB.screenshot(),
      contentType: 'image/png',
    });
    expect(existsSync(localArchivePath(seeded.deviceB.appDataDir, first))).toBe(false);
    expect(await readSave(seeded.deviceB)).toBe(before);
    await session.pageB.route(
      '**/api/v1/defer-progress-notices',
      (route) =>
        route.fulfill({
          status: 500,
          contentType: 'application/json',
          body: JSON.stringify({ code: 'internal', message: 'test interruption' }),
        }),
      { times: 1 }
    );
    await prompt.getByRole('button', { name: 'Later', exact: true }).click();
    await expect(session.pageB.getByText(/test interruption/).first()).toBeAttached();
    await expect(prompt).toBeVisible();
    await prompt.getByRole('button', { name: 'Later', exact: true }).click();
    await expect(prompt).toBeHidden();
    await refreshB();
    await expect(prompt).toBeHidden();
    await session.pageB.reload();
    await openApp(session.pageB);
    await refreshB();
    await expect(prompt).toBeHidden();

    await openGame(session.pageB);
    await session.pageB.getByRole('button', { name: 'View managed files' }).click();
    const drawer = session.pageB.getByRole('dialog');
    const path = drawer.locator('[contenteditable="true"]').last();
    const draft = `${seeded.deviceB.savePath.replaceAll('\\', '/')}.draft`;
    await path.fill(draft);
    await writeSave(seeded.deviceA, 'new progress on A\n');
    const next = await createPublishedSnapshot(session.pageA, session.hostA, 'Next on A');
    await refreshB();
    await expect(prompt).toBeHidden();
    await expect(path).toHaveText(draft);
    await drawer.getByRole('button', { name: 'Close', exact: true }).click();
    await expect(prompt).toBeVisible();
    expect(existsSync(localArchivePath(seeded.deviceB.appDataDir, next))).toBe(false);
    expect(await readSave(seeded.deviceB)).toBe(before);
    await prompt.getByRole('button', { name: 'Compare', exact: true }).click();
    const comparison = session.pageB.getByRole('dialog').filter({ hasText: 'Next on A' });
    await expect(comparison.getByText('Synced', { exact: true })).toHaveCount(0);
    await comparison.getByRole('button', { name: 'Use this progress', exact: true }).click();
    // Only the final explicit confirmation may replace the live save.
    expect(await readSave(seeded.deviceB)).toBe(before);
    await session.pageB
      .getByRole('dialog', { name: "Apply another device's progress?" })
      .getByRole('button', { name: 'Use this progress', exact: true })
      .click();
    await expect.poll(() => readSave(seeded.deviceB)).toBe('new progress on A\n');
    expect(existsSync(localArchivePath(seeded.deviceB.appDataDir, next))).toBe(true);
    await refreshB();
    await expect(prompt).toBeHidden();
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
