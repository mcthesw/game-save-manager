import { spawnSync, type ChildProcess } from 'node:child_process';
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { createWriteStream, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { finished } from 'node:stream/promises';
import { hostCommand, spawnTestProcess } from './process';
import { buildEnvironment, prepareBuild, preparedBinary } from './build-state';
import { reportTiming } from './timing';
import { waitForHttpHost } from '../../scripts/wait-http-host';

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const workspaceRoot = resolve(appRoot, '../..');

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
    { cwd: workspaceRoot, stdio: 'inherit', windowsHide: true }
  );
  if (result.status !== 0) {
    throw new Error(`failed to compile e2e xxh3 helper with code ${result.status}`);
  }
}

export function prepareRgsmBuild(): void {
  const startedAt = performance.now();
  prepareBuild(process.env, buildPaths(), () => {
    const result = spawnSync('cargo', ['build', '--locked', '-p', 'rgsm', '--bin', 'rgsm'], {
      cwd: workspaceRoot,
      stdio: 'inherit',
      windowsHide: true,
      env: buildEnvironment(process.env),
    });
    if (result.error) throw result.error;
    if (result.status !== 0) {
      throw new Error(`E2E host build failed with code ${result.status}`);
    }
    compileXxh3Helper();
  });
  reportTiming('host and helper build', startedAt);
}

function buildPaths() {
  return { binary: rgsmBinaryPath(), helper: xxh3HelperPath() };
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
    spawnSync('taskkill', ['/pid', String(child.pid), '/T', '/F'], {
      stdio: 'ignore',
      windowsHide: true,
    });
    return;
  }
  child.kill('SIGTERM');
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
  const binary = preparedBinary(process.env, buildPaths());
  const startedAt = performance.now();
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

  const command = hostCommand(binary, process.platform);
  const owned = spawnTestProcess(command.command, command.args, { cwd: workspaceRoot, env });
  const { child } = owned;
  let outputTail = '';
  const capture = (chunk: Buffer) => {
    outputTail = (outputTail + chunk.toString('utf8')).slice(-16_384);
  };
  child.stdout.on('data', capture);
  child.stderr.on('data', capture);
  child.stdout.pipe(log, { end: false });
  child.stderr.pipe(log, { end: false });
  let stopping: Promise<void> | undefined;
  const stop = () =>
    (stopping ??= (async () => {
      try {
        await owned.stop();
      } finally {
        log.end();
        await finished(log);
      }
    })());

  try {
    const runtime = await waitForHttpHost(options.appDataDir, {
      timeoutMs: options.readyTimeoutMs,
      failure: owned.failure,
    });
    reportTiming(`HTTP host ready (${options.deviceId})`, startedAt);
    return {
      ...runtime,
      appDataDir: options.appDataDir,
      deviceId: options.deviceId,
      logPath: options.logPath,
      stop,
    };
  } catch (error) {
    await stop();
    const tail = outputTail.trim() || (await readLogTail(options.logPath));
    throw new Error(`RGSM Host for ${options.deviceId} failed.\n${tail}`, { cause: error });
  }
}

export async function newDeviceContext(
  browser: Browser,
  host: RgsmHost
): Promise<{ context: BrowserContext; page: Page }> {
  const context = await browser.newContext();
  try {
    await context.addInitScript(
      ({ apiBaseUrl, token }) => {
        window.__RGSM_RUNTIME__ = { apiBaseUrl, token };
      },
      { apiBaseUrl: host.apiBaseUrl, token: host.token }
    );
    const page = await context.newPage();
    return { context, page };
  } catch (error) {
    await context.close();
    throw error;
  }
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
    signal: AbortSignal.timeout(30_000),
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
