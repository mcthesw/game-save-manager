import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { assertNextRelease } from "./release-policy.mjs";

export function requestedReleaseStrategy(requireRelease, requested) {
  requireRelease("./build/src/index.js");
  const { Simple } = requireRelease("./build/src/strategies/simple.js");
  const { Version } = requireRelease("./build/src/version.js");
  const newVersion = Version.parse(requested);
  // The requested version must open a PR even without a new user-facing commit.
  return class RequestedRelease extends Simple {
    buildReleasePullRequest(commits, latestRelease, draft, labels) {
      return super.buildReleasePullRequest(
        commits,
        latestRelease,
        draft,
        labels,
        { newVersion },
      );
    }
  };
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  const version = process.env.RELEASE_VERSION;
  const previous = JSON.parse(
    readFileSync(".release-please-manifest.json", "utf8"),
  )["."];
  assertNextRelease(previous, version);
  const requireRelease = createRequire(
    resolve(process.env.RELEASE_PLEASE_ROOT, "package.json"),
  );
  const { GitHub, Manifest, registerReleaseType } = requireRelease(
    "./build/src/index.js",
  );
  const Strategy = requestedReleaseStrategy(requireRelease, version);
  registerReleaseType("simple", (options) => new Strategy(options));
  const [owner, repo] = process.env.GITHUB_REPOSITORY.split("/");
  if (!process.env.GH_TOKEN)
    throw new Error("RELEASE_TOKEN is required so the release PR triggers CI");
  const github = await GitHub.create({
    owner,
    repo,
    token: process.env.GH_TOKEN,
  });
  const manifest = await Manifest.fromManifest(github, "dev");
  await manifest.createPullRequests();
}
