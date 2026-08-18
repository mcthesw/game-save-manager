import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { DEVICE_A_ID, DEVICE_B_ID, deviceProfileFileName } from './support/constants';
import {
  createRunRoot,
  hostPost,
  newDeviceContext,
  removeRunRoot,
  startRgsmHost,
  startTestWeb,
} from './support/rgsm-instance';
import { cloudSyncNav, getCurrentDevice } from './support/gui';
import { localOwnerPaths } from './support/cloud-assertions';

test('A/B Hosts isolate device id, token, port, and browser traffic', async ({ browser }) => {
  const runRoot = await createRunRoot('isolation');
  const vite = await startTestWeb();
  const hostA = await startRgsmHost({
    appDataDir: join(runRoot, 'app-data-a'),
    deviceId: DEVICE_A_ID,
    logPath: join(runRoot, 'logs', 'host-a.log'),
  });
  const hostB = await startRgsmHost({
    appDataDir: join(runRoot, 'app-data-b'),
    deviceId: DEVICE_B_ID,
    logPath: join(runRoot, 'logs', 'host-b.log'),
  });

  try {
    const deviceA = await getCurrentDevice(hostA);
    const deviceB = await getCurrentDevice(hostB);
    expect(deviceA.id).toBe(DEVICE_A_ID);
    expect(deviceB.id).toBe(DEVICE_B_ID);
    expect(hostA.token).not.toBe(hostB.token);
    expect(hostA.port).not.toBe(hostB.port);
    expect(hostA.appDataDir).not.toBe(hostB.appDataDir);

    expect(existsSync(localOwnerPaths(hostA.appDataDir, DEVICE_A_ID).profile)).toBe(true);
    expect(existsSync(localOwnerPaths(hostB.appDataDir, DEVICE_B_ID).profile)).toBe(true);
    expect(
      existsSync(join(hostA.appDataDir, 'device-profiles', deviceProfileFileName(DEVICE_B_ID)))
    ).toBe(false);

    const rejected = await fetch(`${hostA.apiBaseUrl}/api/v1/get-build-info`, {
      method: 'POST',
      headers: { Authorization: `Bearer ${hostB.token}` },
    });
    expect(rejected.status).toBe(401);

    const seenA: string[] = [];
    const seenB: string[] = [];
    const { context: contextA, page: pageA } = await newDeviceContext(browser, hostA);
    const { context: contextB, page: pageB } = await newDeviceContext(browser, hostB);
    pageA.on('request', (request) => seenA.push(request.url()));
    pageB.on('request', (request) => seenB.push(request.url()));
    await pageA.goto('/');
    await pageB.goto('/');
    await expect(cloudSyncNav(pageA)).toBeVisible();
    await expect(cloudSyncNav(pageB)).toBeVisible();

    expect(seenA.some((url) => url.startsWith(hostA.apiBaseUrl))).toBe(true);
    expect(seenB.some((url) => url.startsWith(hostB.apiBaseUrl))).toBe(true);
    expect(seenA.some((url) => url.startsWith(hostB.apiBaseUrl))).toBe(false);
    expect(seenB.some((url) => url.startsWith(hostA.apiBaseUrl))).toBe(false);

    const infoA = await hostPost<{ token?: string }>(hostA, '/api/v1/get-http-host-info');
    const infoB = await hostPost<{ token?: string }>(hostB, '/api/v1/get-http-host-info');
    expect(infoA.ok).toBe(true);
    expect(infoB.ok).toBe(true);

    await contextA.close();
    await contextB.close();
  } finally {
    await hostA.stop();
    await hostB.stop();
    await vite.stop();
    await removeRunRoot(runRoot);
  }
});
