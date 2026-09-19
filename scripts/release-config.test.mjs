import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import test from "node:test";
import { requestedReleaseStrategy } from "./prepare-release.mjs";

// Exercise the pinned updater against real manifests, including TOML array
// selection in Cargo.lock. A syntactically valid config is not enough.
const requireRelease = createRequire(
  resolve(process.env.RELEASE_PLEASE_ROOT, "package.json"),
);
const { GenericToml } = requireRelease("./build/src/updaters/generic-toml.js");
const { Version } = requireRelease("./build/src/version.js");
const toml = requireRelease("@iarna/toml");
const config = JSON.parse(readFileSync("release-please-config.json", "utf8"))
  .packages["."];

test("a requested stable release opens without inventing another code change", async () => {
  const { TagName } = requireRelease("./build/src/util/tag-name.js");
  const Strategy = requestedReleaseStrategy(requireRelease, "1.9.0");
  const strategy = new Strategy({
    github: { repository: { owner: "mcthesw", repo: "game-save-manager" } },
    targetBranch: "dev",
    includeComponentInTag: false,
    versionFile: config["version-file"],
    extraFiles: config["extra-files"],
  });
  const pr = await strategy.buildReleasePullRequest([], {
    tag: new TagName(Version.parse("1.8.0")),
    sha: "a".repeat(40),
    notes: "Previous release",
  });
  assert.equal(pr.version.toString(), "1.9.0");
  assert.deepEqual(
    pr.updates.map((update) => update.path),
    ["CHANGELOG.md", ".release-version", "Cargo.toml", "Cargo.lock"],
  );
});

test("release updates preserve inherited versions and unrelated lockfile packages", () => {
  const files = new Map(
    config["extra-files"].map(({ path }) => [path, readFileSync(path, "utf8")]),
  );
  const original = toml.parse(files.get("Cargo.toml"));
  const members = original.workspace.members.map(
    (path) => toml.parse(readFileSync(`${path}/Cargo.toml`, "utf8")).package,
  );
  assert.ok(members.every((member) => member.version.workspace));
  const memberNames = new Set(members.map((member) => member.name));
  const unrelated = (source) =>
    toml.parse(source).package.filter((entry) => !memberNames.has(entry.name));
  const originalDependencies = unrelated(files.get("Cargo.lock"));
  for (const value of ["1.9.0", "1.9.1"]) {
    const version = Version.parse(value);
    for (const file of config["extra-files"]) {
      files.set(
        file.path,
        new GenericToml(file.jsonpath, version).updateContent(
          files.get(file.path),
        ),
      );
    }
    assert.equal(
      toml.parse(files.get("Cargo.toml")).workspace.package.version,
      value,
    );
    const locked = toml
      .parse(files.get("Cargo.lock"))
      .package.filter((entry) => memberNames.has(entry.name));
    assert.equal(locked.length, members.length);
    assert.ok(locked.every((entry) => entry.version === value));
    assert.deepEqual(unrelated(files.get("Cargo.lock")), originalDependencies);
  }
});
