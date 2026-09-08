import { test, expect } from '@playwright/test';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { seedEmptyCloudWithLocalGame, readSave } from './support/cloud-fixture';
import { startDualSession } from './support/session';
import { startJoinCodeWebDav } from './support/join-code-webdav';
import { createRunRoot, hostPost, startRgsmHost } from './support/rgsm-instance';
import { createLibrary, openSyncSettings, openDeviceSettings, libraryDevices } from './support/gui';
import { DEVICE_A_ID, DEVICE_B_ID, DEVICE_A_NAME } from './support/constants';
import { getLocalGame } from './support/local-gui';

test('cloud join code previews without changes, confirms an existing library, and rejects a replaced target', async ({
  browser,
}, testInfo) => {
  const runRoot = await createRunRoot('join-code');
  const scene = await seedEmptyCloudWithLocalGame(runRoot);
  const dav = await startJoinCodeWebDav();
  for (const device of [scene.deviceA, scene.deviceB]) {
    const path = join(device.appDataDir, 'GameSaveManager.config.json');
    const config = JSON.parse(await readFile(path, 'utf8'));
    if (device === scene.deviceB) {
      config.games[0].name = 'Private game';
      config.games[0].storage_key = 'private-game';
    }
    config.settings.cloud_settings.backend =
      device === scene.deviceA ? dav.backend : { type: 'Disabled' };
    config.settings.cloud_settings.root_path = '/';
    await writeFile(path, JSON.stringify(config));
  }
  let session: Awaited<ReturnType<typeof startDualSession>> | undefined;
  let failed = false;
  try {
    session = await startDualSession(browser, { runRoot, ...scene, label: 'join-code' });
    const { pageA, pageB, hostB } = session;
    await createLibrary(pageA);
    await openSyncSettings(pageA);
    await pageA
      .getByLabel('Sync settings')
      .getByRole('button', { name: 'Cloud settings', exact: true })
      .click();
    const saveButton = pageA.getByRole('button', { name: 'Save', exact: true });
    const joinButton = pageA.getByRole('button', { name: 'Use cloud join code', exact: true });
    const saveBounds = await saveButton.boundingBox();
    const joinBounds = await joinButton.boundingBox();
    expect(saveBounds).not.toBeNull();
    expect(joinBounds).not.toBeNull();
    expect(joinBounds!.y).toBeGreaterThanOrEqual(saveBounds!.y - 1);
    await pageA.screenshot({ path: testInfo.outputPath('cloud-connection-actions.png') });
    const exportedResponse = pageA.waitForResponse((response) =>
      response.url().endsWith('/api/v1/export-cloud-join-code')
    );
    await pageA.getByRole('button', { name: 'Get cloud join code', exact: true }).click();
    expect((await exportedResponse).headers()['cache-control']).toBe('no-store');
    const exported = pageA.getByRole('dialog');
    const codeInput = exported.getByRole('textbox', { name: 'Cloud join code' });
    await expect(codeInput).toHaveValue(/^RGSM1:/);
    const code = await codeInput.inputValue();
    await expect(
      exported.getByText(/Anyone with this code can access and delete cloud saves/)
    ).toBeVisible();
    await exported.getByRole('button', { name: 'cancel', exact: true }).click();
    const before = await hostPost(hostB, '/api/v1/get-local-config');
    const originalPrivateGame = await getLocalGame(hostB, 'Private game');
    // Connecting makes unmatched legacy games explicitly local-only, not shared.
    const retainedPrivateGame = { ...originalPrivateGame, cloud_sync_enabled: false };
    const originalDevice = await hostPost(hostB, '/api/v1/get-current-device-info');
    await openSyncSettings(pageB);
    await pageB
      .getByLabel('Sync settings')
      .getByRole('button', { name: 'Cloud settings', exact: true })
      .click();
    await pageB.getByRole('button', { name: 'Use cloud join code', exact: true }).click();
    const dialog = pageB.getByRole('dialog');
    const writesBefore = dav.writes.length;
    await dialog.getByRole('textbox', { name: 'Cloud join code' }).fill(code);
    await dialog.getByRole('button', { name: 'Preview library', exact: true }).click();
    await expect(dialog.getByText('1 shared games')).toBeVisible();
    await pageB.screenshot({
      path: testInfo.outputPath('acceptance-cloud-join.png'),
      mask: [dialog.getByRole('textbox', { name: 'Cloud join code' })],
    });
    await dialog.getByRole('button', { name: 'cancel', exact: true }).click();
    expect((await hostPost(hostB, '/api/v1/get-local-config')).data).toEqual(before.data);
    expect(dav.writes.length).toBe(writesBefore);
    await pageB.getByRole('button', { name: 'Use cloud join code', exact: true }).click();
    await expect(dialog.getByRole('textbox', { name: 'Cloud join code' })).toHaveValue('');
    await dialog.getByRole('textbox', { name: 'Cloud join code' }).fill(code);
    await dialog.getByRole('button', { name: 'Preview library', exact: true }).click();
    await dialog.getByRole('button', { name: 'Connect this library', exact: true }).click();
    await expect(dialog).toBeHidden();
    const joined = await hostPost<{ games: Array<{ name: string }>; backup_path: string }>(
      hostB,
      '/api/v1/get-local-config'
    );
    expect(joined.data.games.map((game) => game.name)).toContain('Echo Keep');
    expect(joined.data.backup_path.replaceAll('\\', '/')).toBe(
      scene.deviceB.archiveRoot.replaceAll('\\', '/')
    );
    expect(await readSave(scene.deviceA)).toBe(await readSave(scene.deviceB));
    expect(await getLocalGame(hostB, 'Private game')).toEqual(retainedPrivateGame);
    expect((await hostPost(hostB, '/api/v1/get-current-device-info')).data).toEqual(
      originalDevice.data
    );
    const manifestBeforeReuse = dav.files.get('/v2/cloud-manifest.json')!.toString();
    await openDeviceSettings(pageB);
    const profileRow = libraryDevices(pageB)
      .locator('div.border-t')
      .filter({ has: pageB.getByText(DEVICE_A_NAME, { exact: true }) });
    await profileRow.getByRole('button', { name: 'Reuse save locations' }).click();
    const reuseDialog = pageB.getByRole('dialog');
    await expect(
      reuseDialog.getByText(/Check that they work on this computer first/)
    ).toBeVisible();
    await expect(reuseDialog.getByText(/manual sync with automatic backups off/)).toBeVisible();
    await pageB.screenshot({ path: testInfo.outputPath('acceptance-profile-reuse.png') });
    await reuseDialog.getByRole('button', { name: 'Reuse save locations', exact: true }).click();
    await expect
      .poll(async () => {
        const game = await getLocalGame(hostB, 'Echo Keep');
        const unit = game.save_paths[0];
        return unit?.source.type === 'concrete' ? unit.source.paths[DEVICE_B_ID] : undefined;
      })
      .toBe(scene.deviceA.savePath.replaceAll('\\', '/'));
    expect((await hostPost<{ id: string }>(hostB, '/api/v1/get-current-device-info')).data.id).toBe(
      DEVICE_B_ID
    );
    expect(await getLocalGame(hostB, 'Private game')).toEqual(retainedPrivateGame);
    expect(dav.files.get('/v2/cloud-manifest.json')!.toString()).toBe(manifestBeforeReuse);
    expect([...dav.files.keys()].filter((path) => path.includes('/archives/'))).toEqual([]);
    const unconfirmed = await hostPost(hostB, '/api/v1/reuse-cloud-device-locations', {
      deviceId: DEVICE_A_ID,
      confirmed: false,
    });
    expect(unconfirmed.ok).toBe(false);
    const configAfterReuse = await hostPost(hostB, '/api/v1/get-local-config');
    const namespacePath = '/v2/namespace.json';
    const namespace = JSON.parse(dav.files.get(namespacePath)!.toString());
    namespace.library_id = '99999999-9999-4999-8999-999999999999';
    dav.files.set(namespacePath, Buffer.from(JSON.stringify(namespace)));
    const writesAfter = dav.writes.length;
    const rejected = await hostPost(hostB, '/api/v1/import-cloud-join-code', {
      code,
      confirmed: true,
    });
    expect(rejected.ok).toBe(false);
    expect(rejected.raw).not.toContain(dav.backend.password);
    expect(dav.writes.length).toBe(writesAfter);
    expect((await hostPost(hostB, '/api/v1/get-local-config')).data).toEqual(configAfterReuse.data);
    await hostB.stop();
    const restarted = await startRgsmHost({
      appDataDir: scene.deviceB.appDataDir,
      deviceId: DEVICE_B_ID,
      logPath: join(runRoot, 'logs', 'join-code-restarted.log'),
    });
    try {
      expect((await hostPost(restarted, '/api/v1/get-local-config')).data).toEqual(
        configAfterReuse.data
      );
    } finally {
      await restarted.stop();
    }
    for (const host of [session.hostA, hostB, restarted]) {
      const log = await readFile(host.logPath, 'utf8');
      expect(log).not.toContain(code);
      expect(log).not.toContain(dav.backend.password);
    }
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session?.close(failed);
    await dav.close();
  }
});
