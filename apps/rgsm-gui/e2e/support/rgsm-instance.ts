import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { createWriteStream, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { VITE_ORIGIN, VITE_PORT } from './constants';

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const workspaceRoot = resolve(appRoot, '../..');

export type TestWeb = {
  origin: string;
  stop: () => Promise<void>;
};

export type RgsmHost = {
  apiBaseUrl: string;
  token: string;
  port: number;
  appDataDir: string;
  deviceId: string;
  logPath: string;
  stop: () => Promise<void>;
};

export type HostStartOptions = {
  appDataDir: string;
  deviceId: string;
  logPath: string;
  env?: Record<string, string | undefined>;
  readyTimeoutMs?: number;
};

type HostFile = {
  port: number;
  api_token: string;
};

type SharedVite = {
  child: ChildProcess | undefined;
  users: number;
};

let sharedVite: SharedVite | undefined;
let builtBinary: string | undefined;

// The Playwright worker owns the Vite child, but global teardown runs in a
// separate process, so the child PID is recorded here for cross-process cleanup.
const viteMarkerPath = join(tmpdir(), 'rgsm-gui-e2e-vite.json');

export function workspacePath(...parts: string[]): string {
  return resolve(workspaceRoot, ...parts);
}

export function rgsmBinaryPath(): string {
  const name = process.platform === 'win32' ? 'rgsm.exe' : 'rgsm';
  return join(workspaceRoot, 'target', 'debug', name);
}

export function xxh3HelperPath(): string {
  const name = process.platform === 'win32' ? 'e2e-xxh3.exe' : 'e2e-xxh3';
  return join(workspaceRoot, 'target', name);
}

function compileXxh3Helper(): void {
  const deps = join(workspaceRoot, 'target', 'debug', 'deps');
  const entries = readdirSync(deps);
  const rlib = entries.find((name) => name.startsWith('libxxhash_rust-') && name.endsWith('.rlib'));
  if (!rlib) {
    throw new Error('xxhash_rust rlib not found after cargo build');
  }
  const result = spawnSync(
    'rustc',
    [
      '--edition',
      '2021',
      `--extern`,
      `xxhash_rust=${join(deps, rlib)}`,
      '-L',
      deps,
      '-o',
      xxh3HelperPath(),
      join(appRoot, 'e2e', 'support', 'xxh3.rs'),
    ],
    { cwd: workspaceRoot, stdio: 'inherit' }
  );
  if (result.status !== 0) {
    throw new Error(`failed to compile e2e xxh3 helper with code ${result.status}`);
  }
}

export async function buildRgsmBinary(): Promise<string> {
  if (builtBinary) return builtBinary;
  const result = spawnSync('cargo', ['build', '--locked', '-p', 'rgsm'], {
    cwd: workspaceRoot,
    stdio: 'inherit',
    env: process.env,
  });
  if (result.status !== 0) {
    throw new Error(`cargo build --locked -p rgsm failed with code ${result.status}`);
  }
  compileXxh3Helper();
  builtBinary = rgsmBinaryPath();
  return builtBinary;
}

export async function createRunRoot(label: string): Promise<string> {
  const root = join(tmpdir(), `rgsm-gui-e2e-${label}-${Date.now()}-${process.pid}`);
  await mkdir(root, { recursive: true });
  return root;
}

export async function removeRunRoot(root: string): Promise<void> {
  await rm(root, { recursive: true, force: true });
}

export function stopProcessTree(child: ChildProcess | undefined): void {
  if (!child || child.exitCode !== null || child.pid === undefined) return;
  if (process.platform === 'win32') {
    spawnSync('taskkill', ['/pid', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    return;
  }
  child.kill('SIGTERM');
}

async function waitForHttpOk(url: string, timeoutMs: number, label: string): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  let lastError: unknown;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.ok) return;
      lastError = new Error(`${label} returned HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await delay(250);
  }
  throw new Error(`Timed out waiting for ${label}: ${String(lastError)}`);
}

function delay(ms: number): Promise<void> {
  const { promise, resolve } = Promise.withResolvers<void>();
  setTimeout(resolve, ms);
  return promise;
}

export async function startTestWeb(): Promise<TestWeb> {
  if (sharedVite) {
    sharedVite.users += 1;
    return {
      origin: VITE_ORIGIN,
      stop: async () => {
        await releaseTestWeb();
      },
    };
  }

  try {
    const response = await fetch(`${VITE_ORIGIN}/`);
    if (response.ok) {
      // A marker means the listener is a stale orphan from a crashed run; it
      // can die mid-test when its dead parent's stdio closes. Replace it.
      let stalePid: number | undefined;
      try {
        const marker = JSON.parse(await readFile(viteMarkerPath, 'utf8')) as { pid?: number };
        stalePid = marker.pid;
      } catch {
        // No marker: a foreign dev server owns the port; adopt it.
      }
      if (stalePid !== undefined) {
        stopPidTree(stalePid);
        await rm(viteMarkerPath, { force: true });
        await waitForPortFree(10_000);
      } else {
        sharedVite = { child: undefined, users: 1 };
        return {
          origin: VITE_ORIGIN,
          stop: async () => {
            await releaseTestWeb();
          },
        };
      }
    }
  } catch {
    // No leftover Vite on the shared port.
  }

  // Spawn Vite's node binary directly (no cmd/pnpm wrappers): the recorded PID
  // is then the port owner itself, so teardown can kill it reliably.
  const child = spawn(process.execPath, [join(appRoot, 'node_modules', 'vite', 'bin', 'vite.js')], {
    cwd: appRoot,
    env: { ...process.env },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  child.stdout?.on('data', (chunk) => {
    process.stdout.write(`[vite] ${chunk}`);
  });
  child.stderr?.on('data', (chunk) => {
    process.stderr.write(`[vite] ${chunk}`);
  });
  child.once('exit', (code) => {
    if (sharedVite?.child === child) sharedVite = undefined;
    if (code && code !== 0) {
      process.stderr.write(`Vite exited with code ${code}\n`);
    }
  });
  try {
    await waitForHttpOk(`${VITE_ORIGIN}/`, 60_000, `Vite on port ${VITE_PORT}`);
  } catch (error) {
    stopProcessTree(child);
    throw error;
  }
  await writeFile(viteMarkerPath, JSON.stringify({ pid: child.pid }), 'utf8');
  sharedVite = { child, users: 1 };
  return {
    origin: VITE_ORIGIN,
    stop: async () => {
      await releaseTestWeb();
    },
  };
}

async function releaseTestWeb(): Promise<void> {
  if (!sharedVite) return;
  sharedVite.users = Math.max(0, sharedVite.users - 1);
}

async function waitForPortFree(timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      await fetch(`${VITE_ORIGIN}/`);
    } catch {
      return;
    }
    await delay(250);
  }
}

function stopPidTree(pid: number): void {
  if (process.platform === 'win32') {
    spawnSync('taskkill', ['/pid', String(pid), '/T', '/F'], { stdio: 'ignore' });
    return;
  }
  try {
    process.kill(pid, 'SIGTERM');
  } catch {
    // Already gone.
  }
}

export async function stopSharedTestWeb(): Promise<void> {
  const child = sharedVite?.child;
  sharedVite = undefined;
  stopProcessTree(child);
  try {
    const marker = JSON.parse(await readFile(viteMarkerPath, 'utf8')) as { pid?: number };
    if (marker.pid !== undefined && marker.pid !== child?.pid) {
      stopPidTree(marker.pid);
    }
  } catch {
    // No marker: nothing this suite spawned is still alive.
  }
  await rm(viteMarkerPath, { force: true });
  await waitForPortFree(10_000);
}

async function readLogTail(logPath: string): Promise<string> {
  try {
    const content = await readFile(logPath, 'utf8');
    const tail = content.trim().split('\n').slice(-30).join('\n');
    return `Host log tail:\n${tail}`;
  } catch {
    return `No host log at ${logPath}`;
  }
}

export async function startRgsmHost(options: HostStartOptions): Promise<RgsmHost> {
  await mkdir(options.appDataDir, { recursive: true });
  await mkdir(dirname(options.logPath), { recursive: true });
  const binary = await buildRgsmBinary();
  const log = createWriteStream(options.logPath, { flags: 'a' });
  const extraEnv = options.env ?? {};
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    RGSM_HTTP_HOST_ONLY: '1',
    RGSM_E2E_APP_DATA_DIR: options.appDataDir,
    RGSM_E2E_DEVICE_ID: options.deviceId,
    RUST_LOG: extraEnv.RUST_LOG ?? 'info',
  };
  for (const [key, value] of Object.entries(extraEnv)) {
    if (value === undefined) delete env[key];
    else env[key] = value;
  }

  const child = spawn(binary, [], {
    cwd: workspaceRoot,
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  // Keep the last bytes of host output in memory: the piped log file is not
  // reliably flushed when the process dies instantly (e.g. startup panic).
  let outputTail = '';
  const capture = (chunk: Buffer) => {
    outputTail = (outputTail + chunk.toString('utf8')).slice(-16384);
  };
  child.stdout?.on('data', capture);
  child.stderr?.on('data', capture);
  child.stdout?.pipe(log);
  child.stderr?.pipe(log);

  const hostConfigPath = join(options.appDataDir, 'GameSaveManager.host.json');
  const timeoutMs = options.readyTimeoutMs ?? 60_000;
  const deadline = Date.now() + timeoutMs;
  let config: HostFile | undefined;
  let lastError: unknown;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) {
      const tail = outputTail.trim() || (await readLogTail(options.logPath));
      throw new Error(
        `RGSM Host for ${options.deviceId} exited with code ${child.exitCode}. Host output tail:\n${tail}`
      );
    }
    try {
      config = JSON.parse(await readFile(hostConfigPath, 'utf8')) as HostFile;
      const response = await fetch(`http://127.0.0.1:${config.port}/api/v1/get-build-info`, {
        method: 'POST',
        headers: { Authorization: `Bearer ${config.api_token}` },
      });
      if (response.ok) break;
      lastError = new Error(`get-build-info returned HTTP ${response.status}`);
      config = undefined;
    } catch (error) {
      lastError = error;
      config = undefined;
    }
    await delay(250);
  }
  if (!config) {
    stopProcessTree(child);
    throw new Error(
      `Timed out waiting for RGSM Host ${options.deviceId}: ${String(lastError)}. See ${options.logPath}`
    );
  }

  let stopped = false;
  return {
    apiBaseUrl: `http://127.0.0.1:${config.port}`,
    token: config.api_token,
    port: config.port,
    appDataDir: options.appDataDir,
    deviceId: options.deviceId,
    logPath: options.logPath,
    stop: async () => {
      if (stopped) return;
      stopped = true;
      stopProcessTree(child);
      await delay(200);
      log.end();
    },
  };
}

export async function newDeviceContext(
  browser: Browser,
  host: RgsmHost
): Promise<{ context: BrowserContext; page: Page }> {
  const context = await browser.newContext();
  await context.addInitScript(
    ({ apiBaseUrl, token }) => {
      window.__RGSM_RUNTIME__ = { apiBaseUrl, token };
    },
    { apiBaseUrl: host.apiBaseUrl, token: host.token }
  );
  const page = await context.newPage();
  return { context, page };
}

export async function hostPost<T>(
  host: RgsmHost,
  path: string,
  body?: unknown
): Promise<{ ok: boolean; status: number; data: T; raw: string }> {
  const response = await fetch(`${host.apiBaseUrl}${path}`, {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${host.token}`,
      ...(body === undefined ? {} : { 'Content-Type': 'application/json' }),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const raw = await response.text();
  let data = undefined as T;
  if (raw) {
    try {
      data = JSON.parse(raw) as T;
    } catch {
      data = raw as T;
    }
  }
  return { ok: response.ok, status: response.status, data, raw };
}

export function fsSession(cloudRoot: string) {
  return {
    root_path: cloudRoot,
    max_concurrency: 1,
    backend: { type: 'Fs' as const },
  };
}

export async function writeJson(path: string, value: unknown): Promise<void> {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}
