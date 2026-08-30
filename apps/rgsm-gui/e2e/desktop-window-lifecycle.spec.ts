import { expect, test, chromium, type Browser, type Page } from '@playwright/test';
import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { createServer } from 'node:net';
import { join } from 'node:path';
import { seedLocalConfig } from './support/local-fixture';
import {
  createRunRoot,
  removeRunRoot,
  rgsmBinaryPath,
  startTestWeb,
  stopProcessTree,
  workspacePath,
} from './support/rgsm-instance';

test.skip(process.platform !== 'win32', 'WebView2 window lifecycle is Windows-specific');

type DesktopProcess = {
  child: ChildProcess;
  output: () => string;
};

async function reservePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      if (!address || typeof address === 'string') {
        server.close();
        reject(new Error('Could not reserve a WebView2 debugging port'));
        return;
      }
      server.close((error) => (error ? reject(error) : resolve(address.port)));
    });
  });
}

function startDesktop(appDataDir: string, deviceId: string, cdpPort: number): DesktopProcess {
  const child = spawn(rgsmBinaryPath(), [], {
    cwd: workspacePath(),
    env: {
      ...process.env,
      RGSM_E2E_APP_DATA_DIR: appDataDir,
      RGSM_E2E_DEVICE_ID: deviceId,
      RGSM_E2E_WEBVIEW_DEBUG_PORT: String(cdpPort),
      RUST_LOG: 'info',
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let output = '';
  const capture = (chunk: Buffer) => {
    output = (output + chunk.toString('utf8')).slice(-16_384);
  };
  child.stdout?.on('data', capture);
  child.stderr?.on('data', capture);
  return { child, output: () => output };
}

async function connectDesktop(
  process: DesktopProcess,
  cdpPort: number
): Promise<{
  browser: Browser;
  page: Page;
}> {
  const endpoint = `http://127.0.0.1:${cdpPort}`;
  await expect
    .poll(
      async () => {
        if (process.child.exitCode !== null) {
          throw new Error(
            `Desktop process exited with code ${process.child.exitCode}:\n${process.output()}`
          );
        }
        try {
          const response = await fetch(`${endpoint}/json/list`);
          if (!response.ok) return false;
          const targets = (await response.json()) as Array<{ url: string }>;
          return targets.some((target) => target.url === 'http://localhost:5173/');
        } catch {
          return false;
        }
      },
      { timeout: 60_000 }
    )
    .toBe(true);

  const browser = await chromium.connectOverCDP(endpoint);
  const page = browser
    .contexts()
    .flatMap((context) => context.pages())
    .find((candidate) => candidate.url() === 'http://localhost:5173/');
  if (!page) {
    await browser.close();
    throw new Error('Desktop WebView page was not available after CDP connected');
  }
  return { browser, page };
}

async function closeDesktopWindow(browser: Browser, process: DesktopProcess): Promise<void> {
  const pid = process.child.pid;
  if (pid === undefined) throw new Error('Desktop process has no PID');
  const close = spawnSync(
    'powershell.exe',
    [
      '-NoProfile',
      '-NonInteractive',
      '-Command',
      `$process = Get-Process -Id ${pid}; if (-not $process.CloseMainWindow()) { exit 1 }`,
    ],
    { stdio: 'pipe', encoding: 'utf8' }
  );
  if (close.status !== 0) {
    throw new Error(`Could not close desktop window: ${close.stderr || close.stdout}`);
  }
  await browser.close().catch(() => undefined);
}

async function readBuildInfo(page: Page): Promise<{ version: string; git_hash: string }> {
  return page.evaluate(async (modulePath) => {
    const { commands } = (await import(modulePath)) as {
      commands: { getBuildInfo: () => Promise<{ version: string; git_hash: string }> };
    };
    return commands.getBuildInfo();
  }, '/src/api/commands.ts');
}

test('desktop recreates a destroyed window with live runtime credentials', async () => {
  const runRoot = await createRunRoot('desktop-window-lifecycle');
  const deviceId = `desktop-window-${process.pid}`;
  const vite = await startTestWeb();
  let failed = false;
  let primary: DesktopProcess | undefined;
  let second: DesktopProcess | undefined;
  let exitDisabled: DesktopProcess | undefined;
  try {
    const trayDevice = await seedLocalConfig(join(runRoot, 'tray-enabled'), {
      settings: { exit_to_tray: true },
    });
    const trayPort = await reservePort();
    primary = startDesktop(trayDevice.appDataDir, deviceId, trayPort);
    let connected = await connectDesktop(primary, trayPort);
    expect((await readBuildInfo(connected.page)).version).toBeTruthy();

    await closeDesktopWindow(connected.browser, primary);
    await expect.poll(() => primary?.child.exitCode).toBeNull();

    second = startDesktop(trayDevice.appDataDir, deviceId, trayPort);
    await expect.poll(() => second?.child.exitCode, { timeout: 20_000 }).toBe(0);
    connected = await connectDesktop(primary, trayPort);
    expect((await readBuildInfo(connected.page)).version).toBeTruthy();
    await connected.browser.close();

    stopProcessTree(primary.child);
    await expect.poll(() => primary?.child.exitCode, { timeout: 20_000 }).not.toBeNull();
    primary = undefined;

    const exitDevice = await seedLocalConfig(join(runRoot, 'tray-disabled'), {
      settings: { exit_to_tray: false },
    });
    const exitPort = await reservePort();
    exitDisabled = startDesktop(exitDevice.appDataDir, `${deviceId}-exit`, exitPort);
    connected = await connectDesktop(exitDisabled, exitPort);
    expect((await readBuildInfo(connected.page)).version).toBeTruthy();
    await closeDesktopWindow(connected.browser, exitDisabled);
    await expect.poll(() => exitDisabled?.child.exitCode, { timeout: 20_000 }).toBe(0);
    exitDisabled = undefined;
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    stopProcessTree(second?.child);
    stopProcessTree(primary?.child);
    stopProcessTree(exitDisabled?.child);
    await vite.stop();
    if (!failed) await removeRunRoot(runRoot);
  }
});
