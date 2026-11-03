import { test, expect } from '@playwright/test';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { seedEmptyCloudWithLocalGame } from './support/cloud-fixture';
import { startDualSession } from './support/session';
import { startJoinCodeWebDav } from './support/join-code-webdav';
import { createRunRoot, hostPost } from './support/rgsm-instance';
import { createLibrary, openSyncSettings } from './support/gui';

test('a throttling provider pauses automatic reads until an explicit retry succeeds', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('cloud-rate-limit');
  const scene = await seedEmptyCloudWithLocalGame(runRoot);
  let throttled = false;
  let requests = 0;
  const dav = await startJoinCodeWebDav(async () => {
    requests += 1;
    return throttled ? 'throttle' : undefined;
  });
  for (const device of [scene.deviceA, scene.deviceB]) {
    const file = join(device.appDataDir, 'GameSaveManager.config.json');
    const config = JSON.parse(await readFile(file, 'utf8'));
    config.settings.cloud_settings.backend = dav.backend;
    config.settings.cloud_settings.root_path = '/';
    await writeFile(file, JSON.stringify(config));
  }
  const session = await startDualSession(browser, { runRoot, ...scene, label: 'cloud-rate-limit' });
  let failed = false;
  const limit = () =>
    hostPost<{ retry_after_secs: number | null }>(session.hostA, '/api/v1/get-cloud-rate-limit');
  try {
    await createLibrary(session.pageA);
    expect((await limit()).data.retry_after_secs).toBeNull();

    throttled = true;
    const refreshed = await hostPost(session.hostA, '/api/v1/refresh-cloud-archive-library');
    expect(refreshed.ok).toBe(false);
    expect(refreshed.raw).toMatch(/limited requests|限制了访问/);
    expect((await limit()).data.retry_after_secs).toBeGreaterThan(0);

    // Opening the page must not send the provider more requests while it throttles.
    const before = requests;
    await session.pageA.goto('/');
    await openSyncSettings(session.pageA);
    await session.pageA.waitForTimeout(3000);
    expect(requests).toBe(before);

    throttled = false;
    const retried = await hostPost(session.hostA, '/api/v1/refresh-cloud-archive-library');
    expect(retried.ok, retried.raw).toBe(true);
    expect((await limit()).data.retry_after_secs).toBeNull();
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
    await dav.close();
  }
});
