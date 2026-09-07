import {
  mkdir,
  readFile,
  realpath,
  lstat,
  rmdir,
  writeFile,
} from "node:fs/promises";
import { createWriteStream } from "node:fs";
import { finished } from "node:stream/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  spawnTestProcess,
  closeResources,
} from "../../apps/rgsm-gui/e2e/support/process.ts";

async function directory(path) {
  await mkdir(path, { recursive: true });
  if ((await lstat(path)).isSymbolicLink())
    throw new Error(`Session directory is a link: ${path}`);
  return path;
}

/** Run trusted, task-specific setup without deleting its data or rediscovering saved PIDs. */
export async function withSession(entryUrl, run) {
  const root = await realpath(dirname(fileURLToPath(entryUrl)));
  const metadata = JSON.parse(
    await readFile(join(root, "session.json"), "utf8"),
  );
  if (metadata.kind !== "rgsm-acceptance")
    throw new Error("Not an acceptance packet");
  const lock = join(root, ".running");
  try {
    await mkdir(lock);
  } catch (error) {
    if (error.code === "EEXIST") {
      throw new Error(
        "Session already running; after an interrupted process, confirm it stopped before removing .running",
      );
    }
    throw error;
  }
  const closers = [];
  const wake = Promise.withResolvers();
  const controller = new AbortController();
  const { signal } = controller;
  let stopping = false;
  let childError;
  const requestStop = () => {
    stopping = true;
    controller.abort(new DOMException("Session stopped", "AbortError"));
    wake.resolve();
  };
  const fail = (error) => {
    if (stopping) return;
    childError ??= error;
    controller.abort(childError);
    wake.resolve();
  };
  process.once("SIGINT", requestStop);
  process.once("SIGTERM", requestStop);
  try {
    const dataDir = await directory(join(root, "data"));
    const logsDir = await directory(join(root, "logs"));
    const evidenceDir = await directory(join(root, "evidence"));
    const context = { root, dataDir, logsDir, evidenceDir, metadata, signal };
    await run({
      ...context,
      defer(close) {
        closers.push(close);
      },
      async prepare(seed) {
        signal.throwIfAborted();
        const marker = join(dataDir, ".prepared");
        try {
          await readFile(marker);
          return;
        } catch (error) {
          if (error.code !== "ENOENT") throw error;
        }
        await seed(context);
        signal.throwIfAborted();
        await writeFile(
          marker,
          "Prepared by run.mjs; restart preserves this data\n",
          { flag: "wx" },
        );
      },
      start(command, args = [], options = {}) {
        signal.throwIfAborted();
        const log = createWriteStream(
          join(logsDir, `process-${closers.length}.log`),
          { flags: "a" },
        );
        const logFinished = finished(log);
        // Attach immediately; stream errors must not become unhandled rejections.
        logFinished.catch(fail);
        let owned;
        closers.push(async () => {
          try {
            await owned?.stop();
          } finally {
            log.end();
            await logFinished;
          }
        });
        owned = spawnTestProcess(command, args, {
          cwd: root,
          ...options,
        });
        owned.child.stdout.pipe(log, { end: false });
        owned.child.stderr.pipe(log, { end: false });
        const failed = () => {
          fail(owned.failure() ?? new Error("Session service exited"));
        };
        owned.child.once("error", failed);
        owned.child.once("exit", failed);
        return owned.child;
      },
      async wait() {
        await wake.promise;
        if (childError) throw childError;
      },
      stop: requestStop,
    });
    if (childError) throw childError;
  } catch (error) {
    if (childError) throw childError;
    if (!(stopping && signal.aborted && error.name === "AbortError"))
      throw error;
  } finally {
    stopping = true;
    process.removeListener("SIGINT", requestStop);
    process.removeListener("SIGTERM", requestStop);
    // Retain the lock if cleanup fails: another launcher must not silently overlap it.
    await closeResources(closers);
    await rmdir(lock);
  }
}
