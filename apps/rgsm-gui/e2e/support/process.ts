import { spawn, spawnSync, type SpawnOptionsWithoutStdio } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';

/** The caller retains this handle; cleanup never discovers processes from saved PIDs. */
export function spawnTestProcess(
  command: string,
  args: string[],
  options: SpawnOptionsWithoutStdio = {}
) {
  const child = spawn(command, args, {
    ...options,
    detached: process.platform !== 'win32',
    windowsHide: true,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let spawnError: Error | undefined;
  child.once('error', (error) => {
    spawnError = error;
  });
  const closed = new Promise<void>((resolve) => child.once('close', () => resolve()));
  let stopping: Promise<void> | undefined;
  function failure(): Error | undefined {
    if (spawnError) return spawnError;
    if (child.exitCode !== null || child.signalCode !== null) {
      return new Error(`${command} exited (${child.exitCode ?? child.signalCode})`);
    }
  }
  async function stopOwnedProcess() {
    const pid = child.pid;
    if (pid === undefined) return;
    if (process.platform === 'win32') {
      if (child.exitCode === null && child.signalCode === null) {
        const result = spawnSync('taskkill', ['/pid', String(pid), '/T', '/F'], {
          stdio: 'ignore',
          windowsHide: true,
          timeout: 5_000,
        });
        if (result.error) throw result.error;
      }
    } else {
      try {
        process.kill(-pid, 'SIGTERM');
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'ESRCH') throw error;
      }
    }
    const finished = await Promise.race([
      closed.then(() => true),
      delay(2_000, false, { ref: false }),
    ]);
    if (!finished) {
      if (process.platform !== 'win32') {
        try {
          process.kill(-pid, 'SIGKILL');
        } catch (error) {
          if ((error as NodeJS.ErrnoException).code !== 'ESRCH') throw error;
        }
      }
      const killed = await Promise.race([
        closed.then(() => true),
        delay(2_000, false, { ref: false }),
      ]);
      if (!killed) throw new Error(`Test process ${pid} did not stop`);
    }
  }
  return {
    child,
    failure,
    stop: () => (stopping ??= stopOwnedProcess()),
  };
}

/** Every acquired resource is closed even when an earlier close fails. */
export async function closeResources(closers: Array<() => Promise<unknown>>): Promise<void> {
  const errors: unknown[] = [];
  for (const close of closers.splice(0).reverse()) {
    try {
      await close();
    } catch (error) {
      errors.push(error);
    }
  }
  if (errors.length) throw new AggregateError(errors, 'Could not close all test resources');
}

/** A Linux host gets its own bus, so duplicate GTK application IDs cannot share one. */
export function hostCommand(binary: string, platform: NodeJS.Platform) {
  return platform === 'linux'
    ? { command: 'dbus-run-session', args: ['--', binary] }
    : { command: binary, args: [] };
}
