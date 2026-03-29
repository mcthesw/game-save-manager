// Modified From https://github.com/zzzgydi/clash-verge/blob/main/scripts/portable.mjs
// GPL-3.0
import { existsSync } from "node:fs";
import { access, readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const scriptPath = fileURLToPath(import.meta.url);
const scriptDir = path.dirname(scriptPath);
export function resolveRepoRoot(startDir = scriptDir) {
  let currentDir = startDir;

  while (true) {
    const cargoTomlPath = path.join(currentDir, "Cargo.toml");
    const guiPackagePath = path.join(
      currentDir,
      "apps",
      "rgsm-gui",
      "package.json",
    );

    if (existsSync(cargoTomlPath) && existsSync(guiPackagePath)) {
      return currentDir;
    }

    const parentDir = path.dirname(currentDir);
    if (parentDir === currentDir) {
      throw new Error("could not resolve repository root");
    }
    currentDir = parentDir;
  }
}

// Lazily resolved so that importing pure helpers (e.g. parseWorkspaceVersion)
// from outside the repository tree does not throw at import time.
let _repoRoot;
export function getRepoRoot() {
  if (_repoRoot === undefined) {
    _repoRoot = resolveRepoRoot();
  }
  return _repoRoot;
}

export function createPortablePaths(rootDir = getRepoRoot()) {
  return {
    releaseDir: path.join(rootDir, "target", "release"),
    cargoTomlPath: path.join(rootDir, "Cargo.toml"),
  };
}

export function parseWorkspaceVersion(cargoToml) {
  let inWorkspacePackage = false;

  for (const rawLine of cargoToml.split(/\r?\n/)) {
    const line = rawLine.trim();

    if (line.startsWith("[") && line.endsWith("]")) {
      inWorkspacePackage = line === "[workspace.package]";
      continue;
    }

    if (!inWorkspacePackage) {
      continue;
    }

    const versionMatch = line.match(/^version\s*=\s*"([^"]+)"/);
    if (versionMatch) {
      return versionMatch[1];
    }
  }

  throw new Error("could not read version from workspace Cargo.toml");
}

async function pathExists(targetPath) {
  try {
    await access(targetPath);
    return true;
  } catch {
    return false;
  }
}

export async function resolvePortable() {
  if (process.platform !== "win32") return;

  const root = getRepoRoot();
  const { releaseDir, cargoTomlPath } = createPortablePaths();
  const requireFromGui = createRequire(
    path.join(root, "apps", "rgsm-gui", "package.json"),
  );
  const AdmZip = requireFromGui("adm-zip");
  const { getOctokit, context } = requireFromGui("@actions/github");

  if (!(await pathExists(releaseDir))) {
    throw new Error("could not find the release dir");
  }

  const zip = new AdmZip();

  zip.addLocalFile(path.join(releaseDir, "rgsm.exe"));
  // zip.addLocalFolder(path.join(releaseDir, "resources"), "resources");

  const cargoToml = await readFile(cargoTomlPath, "utf-8");
  const version = parseWorkspaceVersion(cargoToml);
  const buildVariant = (process.env.RGSM_BUILD_VARIANT?.trim() ?? "").toLowerCase();
  const variantSuffix = buildVariant ? `-${buildVariant}` : "";
  const zipFile = path.join(root, `RGSM_${version}_x64-portable${variantSuffix}.zip`);

  zip.writeZip(zipFile);

  console.log("[INFO]: create portable zip successfully");

  if (process.env.GITHUB_TOKEN === undefined) {
    throw new Error("GITHUB_TOKEN is required");
  }

  const options = { owner: context.repo.owner, repo: context.repo.repo };
  const github = getOctokit(process.env.GITHUB_TOKEN);

  console.log("[INFO]: upload to ", process.env.RELEASE_ID);

  // https://octokit.github.io/rest.js
  await github.rest.repos.uploadReleaseAsset({
    ...options,
    release_id: process.env.RELEASE_ID,
    name: path.basename(zipFile),
    data: zip.toBuffer(),
  });
}

if (
  process.argv[1] &&
  pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url
) {
  resolvePortable().catch(console.error);
}
