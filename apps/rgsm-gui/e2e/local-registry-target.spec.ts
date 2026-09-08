import { test, expect } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { DEVICE_A_ID, GAME_NAME } from './support/constants';
import { seedLocalConfig } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import { createSnapshotForGame, getLocalGame } from './support/local-gui';
import { openGame, snapshotRow, expectActivity } from './support/gui';
import { createRunRoot } from './support/rgsm-instance';

test('registry Apply follows the edited target; a missing target is saved with a warning', async ({
  browser,
}) => {
  test.skip(process.platform !== 'win32', 'Windows registry integration');
  const owned = `HKEY_CURRENT_USER\\Software\\RGSM_TEST_${randomUUID()}`;
  const original = `${owned}\\OldName`;
  const target = `${owned}\\NewName`;
  const registry = (...args: string[]) =>
    execFileSync('reg.exe', args, { windowsHide: true, encoding: 'utf8' });
  const runRoot = await createRunRoot('registry-target');
  const device = await seedLocalConfig(runRoot, {
    games: [{ name: GAME_NAME, units: [{ type: 'WinRegistry', path: original }] }],
    settings: { extra_backup_when_apply: false },
  });
  registry('add', original, '/v', 'Progress', '/d', 'saved-progress', '/f');
  let failed = false;
  let session: Awaited<ReturnType<typeof startLocalSession>> | undefined;
  try {
    session = await startLocalSession(browser, { runRoot, device, label: 'registry-target' });
    const { page, host } = session;
    const snapshot = await createSnapshotForGame(host, GAME_NAME, 'registry snapshot');
    registry('add', original, '/v', 'Progress', '/d', 'live-original', '/f');
    await openGame(page);
    await page.getByRole('button', { name: 'View managed files' }).click();
    const drawer = page.getByRole('dialog');
    await drawer.locator('.pvi-editor').last().fill(target);
    await drawer.getByRole('button', { name: 'save', exact: true }).click();
    await expectActivity(
      page,
      'Some save locations are unavailable on this device; the configuration can still be saved'
    );
    await expect
      .poll(async () => {
        const unit = (await getLocalGame(host, GAME_NAME)).save_paths[0];
        return unit?.source.type === 'concrete' ? unit.source.paths[DEVICE_A_ID] : undefined;
      })
      .toBe(target);
    await snapshotRow(page, snapshot).getByRole('button', { name: 'Apply' }).click();
    await expect
      .poll(() => {
        try {
          return registry('query', target, '/v', 'Progress');
        } catch {
          return '';
        }
      })
      .toContain('saved-progress');
    expect(registry('query', original, '/v', 'Progress')).toContain('live-original');
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session?.close(failed);
    // Only this test's freshly generated subtree is owned by the test.
    registry('delete', owned, '/f');
  }
});
