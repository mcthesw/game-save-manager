import assert from "node:assert/strict";
import test from "node:test";
import {
  assertNextRelease,
  assertReleaseSource,
  assertReleaseChecks,
  assertReleaseAssets,
  REQUIRED_RELEASE_CHECKS,
} from "./release-policy.mjs";

test("release progression accepts only newer stable versions", () => {
  for (const [before, after] of [
    ["1.8.0", "1.9.0"],
    ["1.9.0", "1.9.1"],
    ["1.9.0", "1.10.0"],
  ])
    assert.doesNotThrow(() => assertNextRelease(before, after));
  for (const [before, after] of [
    ["1.9.0", "1.9.0-rc.1"],
    ["1.8.0", "1.9.0-rc.1"],
    ["1.9.0", "1.9.0"],
    ["1.9.0", "1.8.9"],
    ["1.8.0", "1.9.0-beta.1"],
    ["1.8.0", "1.9.0; echo unexpected"],
  ])
    assert.throws(() => assertNextRelease(before, after));
});

test("only a matching stable draft can be published", () => {
  const sha = "a".repeat(40);
  const cargo = '[workspace.package]\nversion = "1.9.0"\n';
  const release = { draft: true, tag_name: "v1.9.0" };
  assert.deepEqual(assertReleaseSource(release, cargo, sha), {
    version: "1.9.0",
  });
  assert.throws(() =>
    assertReleaseSource({ ...release, draft: false }, cargo, sha),
  );
  assert.throws(() =>
    assertReleaseSource({ ...release, tag_name: "v1.9.1" }, cargo, sha),
  );
  assert.throws(() => assertReleaseSource(release, cargo, "dev"));
  assert.throws(() =>
    assertReleaseSource(release, cargo.replace("1.9.0", "1.9.0-rc.1"), sha),
  );
});

test("publication requires current successful checks and all platform artifacts", () => {
  const checks = REQUIRED_RELEASE_CHECKS.map((name, id) => ({
    name,
    id,
    status: "completed",
    conclusion: "success",
  }));
  assert.doesNotThrow(() => assertReleaseChecks(checks));
  assert.throws(() => assertReleaseChecks(checks.slice(1)));
  assert.throws(() =>
    assertReleaseChecks([
      ...checks,
      { ...checks[0], id: 99, conclusion: "failure" },
    ]),
  );
  assert.throws(() =>
    assertReleaseChecks([
      ...checks,
      { ...checks[0], id: 99, status: "in_progress", conclusion: null },
    ]),
  );
  const assets = [
    "LICENSE",
    "RGSM_1.9.0_x64-setup.exe",
    "RGSM_1.9.0_x64_en-US.msi",
    "RGSM_1.9.0_x64-portable-slim.zip",
    "RGSM_1.9.0_amd64.deb",
    "RGSM-1.9.0-1.x86_64.rpm",
    "RGSM_1.9.0_amd64.AppImage",
    "RGSM_1.9.0_aarch64.dmg",
    "RGSM_1.9.0_aarch64.app.tar.gz",
  ].map((name) => ({ name, size: 10 }));
  assert.doesNotThrow(() => assertReleaseAssets(assets, "1.9.0"));
  assert.throws(() => assertReleaseAssets(assets.slice(1), "1.9.0"));
  assert.throws(() =>
    assertReleaseAssets(
      assets.map((asset) => ({ ...asset, size: 0 })),
      "1.9.0",
    ),
  );
  assert.throws(() => assertReleaseAssets(assets, "1.9.1"));
  for (const missing of assets.slice(1))
    assert.throws(() =>
      assertReleaseAssets(
        assets.filter((asset) => asset !== missing),
        "1.9.0",
      ),
    );
});
