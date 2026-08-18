import type { Browser, BrowserContext, Page } from '@playwright/test';
import { join } from 'node:path';
import { DEVICE_A_ID, DEVICE_B_ID } from './constants';
import type { DeviceLayout } from './cloud-fixture';
import {
  newDeviceContext,
  removeRunRoot,
  startRgsmHost,
  startTestWeb,
  type RgsmHost,
} from './rgsm-instance';
import { openApp } from './gui';

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
  const vite = await startTestWeb();
  const hostA = await startRgsmHost({
    appDataDir: options.deviceA.appDataDir,
    deviceId: DEVICE_A_ID,
    logPath: join(options.runRoot, 'logs', `${options.label}-a.log`),
  });
  const hostB = await startRgsmHost({
    appDataDir: options.deviceB.appDataDir,
    deviceId: DEVICE_B_ID,
    logPath: join(options.runRoot, 'logs', `${options.label}-b.log`),
  });
  const startedA = await newDeviceContext(browser, hostA);
  const startedB = await newDeviceContext(browser, hostB);
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
      await startedA.context.close();
      await startedB.context.close();
      await hostA.stop();
      await hostB.stop();
      await vite.stop();
      if (!keepRoot) {
        await removeRunRoot(options.runRoot);
      }
    },
  };
}
