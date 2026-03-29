import assert from "node:assert/strict";
import os from "node:os";
import test from "node:test";
import path from "node:path";

import {
  createPortablePaths,
  getRepoRoot,
  parseWorkspaceVersion,
  resolveRepoRoot,
} from "./portable.mjs";

test("createPortablePaths resolves from repository root instead of cwd", () => {
  const originalCwd = process.cwd();
  const root = getRepoRoot();

  try {
    process.chdir(path.join(root, "apps", "rgsm-gui"));

    const { releaseDir, cargoTomlPath } = createPortablePaths();

    assert.equal(releaseDir, path.join(root, "target", "release"));
    assert.equal(cargoTomlPath, path.join(root, "Cargo.toml"));
  } finally {
    process.chdir(originalCwd);
  }
});

test("createPortablePaths ignores unrelated current working directories", () => {
  const originalCwd = process.cwd();
  const root = getRepoRoot();

  try {
    process.chdir(os.tmpdir());

    const { releaseDir, cargoTomlPath } = createPortablePaths();

    assert.equal(releaseDir, path.join(root, "target", "release"));
    assert.equal(cargoTomlPath, path.join(root, "Cargo.toml"));
  } finally {
    process.chdir(originalCwd);
  }
});

test("resolveRepoRoot walks up from nested directories", () => {
  const nestedDir = path.join(getRepoRoot(), "apps", "rgsm-gui", "src");

  assert.equal(resolveRepoRoot(nestedDir), getRepoRoot());
});

test("parseWorkspaceVersion reads the workspace package version", () => {
  const cargoToml = `
[workspace]
members = []

[workspace.package]
version = "1.8.1"
`;

  assert.equal(parseWorkspaceVersion(cargoToml), "1.8.1");
});

test("parseWorkspaceVersion ignores unrelated version fields", () => {
  const cargoToml = `
[package]
name = "example"
version = "0.1.0"

[dependencies]
foo = { version = "9.9.9" }

[workspace.package]
version = "1.8.1"
`;

  assert.equal(parseWorkspaceVersion(cargoToml), "1.8.1");
});

test("parseWorkspaceVersion handles trailing TOML comments", () => {
  const cargoToml = `
[workspace.package]
version = "2.0.0" # release
`;

  assert.equal(parseWorkspaceVersion(cargoToml), "2.0.0");
});

test("parseWorkspaceVersion throws when version is malformed", () => {
  assert.throws(
    () =>
      parseWorkspaceVersion(`
[workspace.package]
version = "1.8.1
`),
    /could not read version/,
  );

  assert.throws(
    () =>
      parseWorkspaceVersion(`
[workspace.package]
version "1.8.1"
`),
    /could not read version/,
  );
});

test("parseWorkspaceVersion throws when version is missing", () => {
  assert.throws(
    () => parseWorkspaceVersion("[workspace]\nmembers = []\n"),
    /could not read version/,
  );
});
