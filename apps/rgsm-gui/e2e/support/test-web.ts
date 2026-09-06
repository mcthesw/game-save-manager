import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { stripVTControlCharacters } from 'node:util';
import { spawnTestProcess } from './process';
import { waitForHttpOk } from './http-probe';
import { VITE_ORIGIN } from './constants';
import { reportTiming } from './timing';

export function hasServerAddress(output: string, origin: string): boolean {
  return stripVTControlCharacters(output).includes(origin);
}

/** Global setup owns the server for the complete run, including worker restarts. */
export async function startTestWebServer(appRoot: string) {
  const startedAt = performance.now();
  const processHandle = spawnTestProcess(
    process.execPath,
    [join(appRoot, 'node_modules', 'vite', 'bin', 'vite.js')],
    { cwd: appRoot, env: { ...process.env } }
  );
  let output = '';
  const capture = (chunk: Buffer) => {
    const text = chunk.toString('utf8');
    output = (output + text).slice(-16_384);
    process.stdout.write(`[vite] ${text}`);
  };
  processHandle.child.stdout.on('data', capture);
  processHandle.child.stderr.on('data', capture);
  try {
    // Only our child's successful bind can establish ownership. A busy port fails;
    // it never adopts or terminates a user's development server.
    const deadline = Date.now() + 60_000;
    while (!hasServerAddress(output, VITE_ORIGIN)) {
      const failure = processHandle.failure();
      if (failure) throw new Error(`Vite failed to start: ${failure.message}\n${output}`);
      if (Date.now() >= deadline) throw new Error(`Vite did not bind ${VITE_ORIGIN}\n${output}`);
      await delay(50);
    }
    await waitForHttpOk(`${VITE_ORIGIN}/`, Math.max(1, deadline - Date.now()), 'test Vite');
    reportTiming('Vite ready', startedAt);
    return processHandle;
  } catch (error) {
    await processHandle.stop();
    throw error;
  }
}
