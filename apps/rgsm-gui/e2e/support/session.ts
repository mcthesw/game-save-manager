import type { Browser, BrowserContext, Page } from '@playwright/test';
import { join } from 'node:path';
import { DEVICE_A_ID, DEVICE_B_ID } from './constants';
import type { DeviceLayout } from './cloud-fixture';
import { newDeviceContext, removeRunRoot, startRgsmHost, type RgsmHost } from './rgsm-instance';
import { openApp } from './gui';
import { closeResources } from './process';

export type DualSession = {
  runRoot: string;
  cloudRoot: string;
  deviceA: DeviceLayout;
  deviceB: DeviceLayout;
  hostA: RgsmHost;
  hostB: RgsmHost;
  contextA: BrowserContext;
  contextB: BrowserContext;
  pageA: Page;
  pageB: Page;
  close: (keepRoot?: boolean) => Promise<void>;
};

export async function startDualSession(
  browser: Browser,
  options: {
    runRoot: string;
    cloudRoot: string;
    deviceA: DeviceLayout;
    deviceB: DeviceLayout;
    label: string;
  }
): Promise<DualSession> {
  const closers: Array<() => Promise<unknown>> = [];
  try {
    const hostA = await startRgsmHost({
      appDataDir: options.deviceA.appDataDir,
      deviceId: DEVICE_A_ID,
      logPath: join(options.runRoot, 'logs', `${options.label}-a.log`),
    });
    closers.push(hostA.stop);
    const hostB = await startRgsmHost({
      appDataDir: options.deviceB.appDataDir,
      deviceId: DEVICE_B_ID,
      logPath: join(options.runRoot, 'logs', `${options.label}-b.log`),
    });
    closers.push(hostB.stop);
    const startedA = await newDeviceContext(browser, hostA);
    closers.push(() => startedA.context.close());
    const startedB = await newDeviceContext(browser, hostB);
    closers.push(() => startedB.context.close());
    await openApp(startedA.page);
    await openApp(startedB.page);
    return {
      runRoot: options.runRoot,
      cloudRoot: options.cloudRoot,
      deviceA: options.deviceA,
      deviceB: options.deviceB,
      hostA,
      hostB,
      contextA: startedA.context,
      contextB: startedB.context,
      pageA: startedA.page,
      pageB: startedB.page,
      close: async (keepRoot = false) => {
        await closeResources(closers);
        if (!keepRoot) await removeRunRoot(options.runRoot);
      },
    };
  } catch (error) {
    try {
      await closeResources(closers);
    } catch (cleanupError) {
      throw new AggregateError([error, cleanupError], 'Session startup and cleanup failed', {
        cause: cleanupError,
      });
    }
    throw error;
  }
}
