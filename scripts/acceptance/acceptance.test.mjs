import { test } from "node:test";
import assert from "node:assert/strict";
import {
  mkdtemp,
  readFile,
  writeFile,
  mkdir,
  rm,
  access,
  copyFile,
  symlink,
} from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { pathToFileURL } from "node:url";
import { spawn, execFileSync } from "node:child_process";
import { once } from "node:events";
import { setTimeout as delay } from "node:timers/promises";
import { promisify } from "node:util";
import { execFile } from "node:child_process";
import { createPacket } from "./create.mjs";
import { withSession } from "./session.mjs";

async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), "rgsm-acceptance-test-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await writeFile(
    join(root, "session.json"),
    JSON.stringify({ kind: "rgsm-acceptance", target: process.platform }),
  );
  return { root, url: pathToFileURL(join(root, "run.mjs")) };
}

test("stop cancels preparation before wait and does not mark partial data prepared", async (t) => {
  const { root, url } = await fixture(t);
  let child;
  await withSession(url, async (session) => {
    assert.ok(session.signal instanceof AbortSignal);
    child = session.start(process.execPath, [
      "-e",
      'console.log("ready");setInterval(()=>{},1000)',
    ]);
    await once(child.stdout, "data");
    await session.prepare(async ({ dataDir, signal }) => {
      await writeFile(join(dataDir, "save.txt"), "keep partial evidence");
      process.emit("SIGINT");
      await delay(10_000, undefined, { signal });
      assert.fail("cancelled setup must not continue");
    });
  });
  assert.ok(child.exitCode !== null || child.signalCode !== null);
  await assert.rejects(access(join(root, ".running")), { code: "ENOENT" });
  await assert.rejects(access(join(root, "data/.prepared")), {
    code: "ENOENT",
  });
  assert.equal(
    await readFile(join(root, "data/save.txt"), "utf8"),
    "keep partial evidence",
  );
});

test("service failure cancels a pending readiness check", async (t) => {
  const { root, url } = await fixture(t);
  await assert.rejects(
    withSession(url, async (session) => {
      assert.ok(session.signal instanceof AbortSignal);
      session.start(join(root, "missing-service"));
      await delay(10_000, undefined, { signal: session.signal });
    }),
    /ENOENT/,
  );
  await assert.rejects(access(join(root, ".running")), { code: "ENOENT" });
});

test("awaited one-shot preparation succeeds before a long-running service", async (t) => {
  const { url } = await fixture(t);
  await withSession(url, async (session) => {
    const result = await promisify(execFile)(
      process.execPath,
      ["-e", 'console.log("prepared")'],
      {
        windowsHide: true,
        signal: session.signal,
      },
    );
    assert.match(result.stdout, /prepared/);
    const child = session.start(process.execPath, [
      "-e",
      'console.log("ready");setInterval(()=>{},1000)',
    ]);
    await once(child.stdout, "data");
    session.stop();
    await session.wait();
  });
});

test("linked data directories are rejected without modifying their target", async (t) => {
  const { root, url } = await fixture(t);
  const outside = join(root, "outside");
  await mkdir(outside);
  await writeFile(join(outside, "save.txt"), "untouched");
  await symlink(
    outside,
    join(root, "data"),
    process.platform === "win32" ? "junction" : "dir",
  );
  await assert.rejects(
    withSession(url, () => assert.fail("must not seed")),
    /directory is a link/,
  );
  assert.equal(await readFile(join(outside, "save.txt"), "utf8"), "untouched");
});

test("seed once, preserve reviewer edits, and release session ownership", async (t) => {
  const { root, url } = await fixture(t);
  let seeds = 0;
  const run = () =>
    withSession(url, async (session) => {
      await session.prepare(async ({ dataDir }) => {
        seeds++;
        await writeFile(join(dataDir, "save.txt"), "initial");
      });
    });
  await run();
  await writeFile(join(root, "data/save.txt"), "reviewer progress");
  await run();
  assert.equal(seeds, 1);
  assert.equal(
    await readFile(join(root, "data/save.txt"), "utf8"),
    "reviewer progress",
  );
  await assert.rejects(access(join(root, ".running")), { code: "ENOENT" });
});

test("concurrent launch is rejected without touching the active session", async (t) => {
  const { root, url } = await fixture(t);
  await withSession(url, async () => {
    await assert.rejects(
      withSession(url, () => assert.fail("must not run")),
      /already running/,
    );
    await access(join(root, ".running"));
  });
});

test("failed fixture keeps its evidence but is not marked prepared", async (t) => {
  const { root, url } = await fixture(t);
  await assert.rejects(
    withSession(url, (session) =>
      session.prepare(async ({ evidenceDir }) => {
        await writeFile(join(evidenceDir, "failure.txt"), "diagnostic");
        throw new Error("seed failed");
      }),
    ),
    /seed failed/,
  );
  await assert.rejects(access(join(root, "data/.prepared")), {
    code: "ENOENT",
  });
  assert.equal(
    await readFile(join(root, "evidence/failure.txt"), "utf8"),
    "diagnostic",
  );
});

test("owned process stops while an unrelated process remains alive", async (t) => {
  const { root, url } = await fixture(t);
  const outsider = spawn(process.execPath, ["-e", "setInterval(()=>{},1000)"], {
    windowsHide: true,
    stdio: "ignore",
  });
  t.after(async () => {
    if (outsider.exitCode === null && outsider.signalCode === null) {
      const exit = once(outsider, "exit");
      outsider.kill();
      await exit;
    }
  });
  let owned;
  await withSession(url, async (session) => {
    owned = session.start(process.execPath, [
      "-e",
      'console.log("ready");setInterval(()=>{},1000)',
    ]);
    await once(owned.stdout, "data");
    session.stop();
    await session.wait();
  });
  assert.ok(owned.exitCode !== null || owned.signalCode !== null);
  assert.equal(outsider.exitCode, null);
  assert.equal(outsider.signalCode, null);
  assert.match(
    await readFile(join(root, "logs/process-0.log"), "utf8"),
    /ready/,
  );
});

test("child startup failure rejects wait and releases ownership", async (t) => {
  const { root, url } = await fixture(t);
  await assert.rejects(
    withSession(url, async (session) => {
      session.start(join(root, "nonexistent-executable"));
      await session.wait();
    }),
    /ENOENT/,
  );
  await assert.rejects(access(join(root, ".running")), { code: "ENOENT" });
});

test("setup failure closes services and preserves files", async (t) => {
  const { root, url } = await fixture(t);
  let child;
  await assert.rejects(
    withSession(url, async (session) => {
      child = session.start(process.execPath, [
        "-e",
        'console.log("ready");setInterval(()=>{},1000)',
      ]);
      await once(child.stdout, "data");
      await writeFile(join(session.dataDir, "save.txt"), "keep");
      throw new Error("readiness failed");
    }),
    /readiness failed/,
  );
  assert.ok(child.exitCode !== null || child.signalCode !== null);
  assert.equal(await readFile(join(root, "data/save.txt"), "utf8"), "keep");
});

test("packet generator records provenance, rejects traversal and never overwrites", async (t) => {
  const { root } = await fixture(t);
  const git = (...args) =>
    execFileSync("git", args, { cwd: root, stdio: "pipe", windowsHide: true });
  git("init");
  git(
    "-c",
    "user.name=Test",
    "-c",
    "user.email=test@example.invalid",
    "commit",
    "--allow-empty",
    "-m",
    "test",
  );
  const packet = await createPacket("linux-paths", {
    repoRoot: root,
    target: "linux",
  });
  const meta = JSON.parse(await readFile(join(packet, "session.json"), "utf8"));
  assert.equal(meta.target, "linux");
  assert.equal(meta.preparedOn, process.platform);
  assert.equal(meta.revision, git("rev-parse", "HEAD").toString().trim());
  assert.equal(meta.dirty, true);
  await writeFile(join(packet, "ACCEPTANCE.md"), "reviewer edits");
  await assert.rejects(createPacket("linux-paths", { repoRoot: root }), {
    code: "EEXIST",
  });
  assert.equal(
    await readFile(join(packet, "ACCEPTANCE.md"), "utf8"),
    "reviewer edits",
  );
  await assert.rejects(
    createPacket("../outside", { repoRoot: root }),
    /lowercase/,
  );
  await assert.rejects(
    createPacket("bad-platform", { repoRoot: root, target: "unknown" }),
    /Target/,
  );
  const example = await createPacket("cloud-example", {
    repoRoot: root,
    target: "win32",
    example: true,
  });
  const recipe = await readFile(join(example, "run.mjs"), "utf8");
  assert.match(recipe, /cloud-fixture\.ts/);
  assert.match(recipe, /startExampleDevice/);
  assert.doesNotMatch(recipe, /__[A-Z_]+__/);
  assert.match(
    await readFile(join(example, "ACCEPTANCE.md"), "utf8"),
    /Not run/,
  );
  await assert.rejects(
    createPacket("unsupported-example", {
      repoRoot: root,
      target: "linux",
      example: true,
    }),
    /win32/,
  );
});

test("generated packet runs on its target, resumes edits, and rejects a different platform", async (t) => {
  const { root } = await fixture(t);
  const git = (...args) =>
    execFileSync("git", args, { cwd: root, stdio: "pipe", windowsHide: true });
  git("init");
  git(
    "-c",
    "user.name=Test",
    "-c",
    "user.email=test@example.invalid",
    "commit",
    "--allow-empty",
    "-m",
    "test",
  );
  await mkdir(join(root, "scripts/acceptance"), { recursive: true });
  await mkdir(join(root, "apps/rgsm-gui/e2e/support"), { recursive: true });
  await copyFile(
    new URL("./session.mjs", import.meta.url),
    join(root, "scripts/acceptance/session.mjs"),
  );
  await copyFile(
    new URL("../../apps/rgsm-gui/e2e/support/process.ts", import.meta.url),
    join(root, "apps/rgsm-gui/e2e/support/process.ts"),
  );
  const packet = await createPacket("local-progress", { repoRoot: root });
  const run = (path) =>
    execFileSync(process.execPath, [join(path, "run.mjs")], {
      encoding: "utf8",
      windowsHide: true,
      stdio: "pipe",
    });
  assert.match(run(packet), /no application was started/);
  await writeFile(join(packet, "data/progress.txt"), "modified in acceptance");
  run(packet);
  assert.equal(
    await readFile(join(packet, "data/progress.txt"), "utf8"),
    "modified in acceptance",
  );
  const other = process.platform === "linux" ? "win32" : "linux";
  const foreign = await createPacket("other-platform", {
    repoRoot: root,
    target: other,
  });
  assert.throws(() => run(foreign), /Run this packet on/);
  await assert.rejects(access(join(foreign, "data/.prepared")), {
    code: "ENOENT",
  });
});
