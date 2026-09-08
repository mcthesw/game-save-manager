import { createWriteStream } from 'node:fs';
import { once } from 'node:events';
import { finished } from 'node:stream/promises';
import { spawnTestProcess } from '../../e2e/support/process.ts';

/** Await a one-shot build; cancellation also stops compiler subprocesses. */
export async function runBuild(command, args, { signal, logPath, ...options }) {
  signal.throwIfAborted();
  const log = createWriteStream(logPath);
  const logDone = finished(log);
  logDone.catch(() => {}); // The finally block observes this even if spawning fails.
  let owned;
  try {
    owned = spawnTestProcess(command, args, options);
    owned.child.stdout.pipe(log, { end: false });
    owned.child.stderr.pipe(log, { end: false });
    const [code] = await Promise.race([
      once(owned.child, 'close', { signal }),
      logDone.then(() => {
        throw new Error('Build log closed unexpectedly');
      }),
    ]);
    if (code !== 0) throw new Error(`Build exited (${code}); see ${logPath}`);
  } finally {
    try {
      await owned?.stop();
    } finally {
      log.end();
      await logDone;
    }
  }
}
