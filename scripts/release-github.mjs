import { execFileSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import { setTimeout as delay } from "node:timers/promises";
import {
  assertNextRelease,
  assertReleaseAssets,
  assertReleaseChecks,
  assertReleaseSource,
  parseReleaseVersion,
} from "./release-policy.mjs";

const repository = process.env.GITHUB_REPOSITORY;
const tag = process.env.RELEASE_TAG;
if (!repository || !tag?.startsWith("v"))
  throw new Error("Repository and version tag are required");
parseReleaseVersion(tag.slice(1));

function run(command, args) {
  return execFileSync(command, args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}
function api(path, extra = []) {
  return JSON.parse(
    run("gh", ["api", `repos/${repository}/${path}`, ...extra]),
  );
}

// Resolve the tag again on retries and before publication. Never build the
// current dev tip merely because a reusable workflow was invoked from it.
function resolveTag() {
  run("git", ["fetch", "--no-tags", "origin", `refs/tags/${tag}`]);
  return run("git", ["rev-parse", "FETCH_HEAD^{commit}"]);
}
const sha = resolveTag();
if (process.env.RELEASE_SHA && sha !== process.env.RELEASE_SHA)
  throw new Error("Release tag moved during the build");
const release = api(`releases/tags/${tag}`);
const { version } = assertReleaseSource(
  release,
  run("git", ["show", `${sha}:Cargo.toml`]),
  sha,
);

if (process.argv[2] === "prepare") {
  appendFileSync(process.env.GITHUB_OUTPUT, `id=${release.id}\nsha=${sha}\n`);
} else if (process.argv[2] === "publish") {
  // Builds and the ordinary dev checks run concurrently. Missing, failed or
  // cancelled checks keep the Release private; a draft can be retried later.
  const deadline = Date.now() + 20 * 60 * 1000;
  while (true) {
    const pages = api(`commits/${sha}/check-runs?per_page=100`, [
      "--paginate",
      "--slurp",
    ]);
    try {
      assertReleaseChecks(pages.flatMap((page) => page.check_runs));
      break;
    } catch (error) {
      if (!error.pending || Date.now() >= deadline) throw error;
      console.log(`${error.message}; waiting for the release commit checks`);
      await delay(30_000);
    }
  }
  run("gh", [
    "release",
    "upload",
    tag,
    "LICENSE",
    "--clobber",
    "--repo",
    repository,
  ]);
  const complete = api(`releases/tags/${tag}`);
  assertReleaseSource(complete, run("git", ["show", `${sha}:Cargo.toml`]), sha);
  assertReleaseAssets(complete.assets, version);
  if (resolveTag() !== sha)
    throw new Error("Release tag moved during publication");
  assertNextRelease(api("releases/latest").tag_name.replace(/^v/, ""), version);
  run("gh", [
    "release",
    "edit",
    tag,
    "--repo",
    repository,
    "--draft=false",
    "--prerelease=false",
    "--latest=true",
  ]);
} else {
  throw new Error("Expected prepare or publish");
}
