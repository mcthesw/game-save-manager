import type { Browser, BrowserContext, Page } from '@playwright/test';
import { join } from 'node:path';
import { DEVICE_A_ID } from './constants';
import type { DeviceLayout } from './cloud-fixture';
import {
  newDeviceContext,
  removeRunRoot,
  startRgsmHost,
  startTestWeb,
  type RgsmHost,
} from './rgsm-instance';
import { openApp } from './gui';

export type LocalSession = {
  runRoot: string;
  device: DeviceLayout;
  host: RgsmHost;
  context: BrowserContext;
  page: Page;
  close: (keepRoot?: boolean) => Promise<void>;
};

/**
 * Starts a single-device, cloud-disabled session. The caller seeds the config
 * (see `seedLocalConfig`) and save files before calling this.
 */
export async function startLocalSession(
  browser: Browser,
  options: { runRoot: string; device: DeviceLayout; label: string }
): Promise<LocalSession> {
  const vite = await startTestWeb();
  const host = await startRgsmHost({
    appDataDir: options.device.appDataDir,
    deviceId: DEVICE_A_ID,
    logPath: join(options.runRoot, 'logs', `${options.label}.log`),
  });
  const started = await newDeviceContext(browser, host);
  await openApp(started.page);
  return {
    runRoot: options.runRoot,
    device: options.device,
    host,
    context: started.context,
    page: started.page,
    close: async (keepRoot = false) => {
      await started.context.close();
      await host.stop();
      await vite.stop();
      if (!keepRoot) {
        await removeRunRoot(options.runRoot);
      }
    },
  };
}
