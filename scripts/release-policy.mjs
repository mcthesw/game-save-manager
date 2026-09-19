import { parseWorkspaceVersion } from "./portable.mjs";

export function parseReleaseVersion(value) {
  const match = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.exec(value);
  if (!match) throw new Error("Use a stable X.Y.Z version");
  return match.slice(1, 4).map(Number);
}

export function assertNextRelease(previous, requested) {
  const before = parseReleaseVersion(previous);
  const next = parseReleaseVersion(requested);
  const different = next.findIndex((part, index) => part !== before[index]);
  if (different < 0 || next[different] < before[different])
    throw new Error(
      "The requested release must be newer than the last release",
    );
  return next;
}

export function assertReleaseSource(release, cargoToml, taggedSha) {
  if (!release.draft)
    throw new Error("Published releases are immutable; prepare a new version");
  const version = parseWorkspaceVersion(cargoToml);
  parseReleaseVersion(version);
  if (release.tag_name !== `v${version}`)
    throw new Error("Release tag and application version differ");
  if (!/^[a-f0-9]{40}$/.test(taggedSha))
    throw new Error("Release must resolve to an exact commit");
  return { version };
}

export const REQUIRED_RELEASE_CHECKS = [
  "Backend Tests",
  "Style & Lint",
  "GUI Cloud Fs E2E",
  "Windows Release E2E",
];

export function assertReleaseChecks(checks) {
  for (const name of REQUIRED_RELEASE_CHECKS) {
    const matching = checks.filter((check) => check.name === name);
    const latest = matching.sort((a, b) => b.id - a.id)[0];
    if (
      !latest ||
      latest.status !== "completed" ||
      latest.conclusion !== "success"
    ) {
      const error = new Error(`Release check has not passed: ${name}`);
      error.pending = !latest || latest.status !== "completed";
      throw error;
    }
  }
}

export function assertReleaseAssets(assets, version) {
  const names = new Set(
    assets.filter((asset) => asset.size > 0).map((asset) => asset.name),
  );
  parseReleaseVersion(version);
  const required = [
    `RGSM_${version}_x64-setup.exe`,
    `RGSM_${version}_x64_en-US.msi`,
    `RGSM_${version}_x64-portable-slim.zip`,
    `RGSM_${version}_amd64.deb`,
    `RGSM-${version}-1.x86_64.rpm`,
    `RGSM_${version}_amd64.AppImage`,
    `RGSM_${version}_aarch64.dmg`,
    `RGSM_${version}_aarch64.app.tar.gz`,
    "LICENSE",
  ];
  for (const name of required) {
    if (!names.has(name)) throw new Error(`Missing release asset: ${name}`);
  }
}
