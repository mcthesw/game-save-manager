import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, rm, realpath } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { validateDataDir, validateOptions } from "./device.mjs";

test("requires an explicit test identity and non-privileged port", () => {
  assert.doesNotThrow(() =>
    validateOptions({ deviceId: "review-b", port: 5188 }),
  );
  for (const port of [0, 80, 65536, NaN, 1234.5])
    assert.throws(() => validateOptions({ deviceId: "review-b", port }));
  for (const deviceId of ["", "../real-device", undefined])
    assert.throws(() => validateOptions({ deviceId, port: 5188 }));
});

test("only launches prepared data inside the task area", async () => {
  const root = await mkdtemp(join(tmpdir(), "rgsm-launcher-test-"));
  try {
    const data = join(root, ".rgsm-dev/acceptance/task/device");
    await mkdir(data, { recursive: true });
    await assert.rejects(validateDataDir(data, root));
    await writeFile(join(data, "GameSaveManager.config.json"), "{}");
    assert.equal(await validateDataDir(data, root), await realpath(data));
    for (const outside of [
      root,
      join(root, ".rgsm-dev/acceptance"),
      join(root, ".rgsm-dev/acceptance-other"),
    ]) {
      await mkdir(outside, { recursive: true });
      await assert.rejects(validateDataDir(outside, root));
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
