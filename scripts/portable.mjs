// Modified From https://github.com/zzzgydi/clash-verge/blob/main/scripts/portable.mjs
// GPL-3.0
import fs from "fs-extra";
import path from "path";
import AdmZip from "adm-zip";
import { createRequire } from "module";
import { getOctokit, context } from "@actions/github";

async function resolvePortable() {
  if (process.platform !== "win32") return;

  const releaseDir = "./src-tauri/target/release";

  if (!(await fs.pathExists(releaseDir))) {
    throw new Error("could not found the release dir");
  }

  const zip = new AdmZip();

  zip.addLocalFile(path.join(releaseDir, "rgsm.exe"));
  // zip.addLocalFolder(path.join(releaseDir, "resources"), "resources");

  const require = createRequire(import.meta.url);
  const packageJson = require("../package.json");
  const { version } = packageJson;

  const zipFile = `RGSM_${version}_x64-portable.zip`;
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
    name: zipFile,
    data: zip.toBuffer(),
  });
}

resolvePortable().catch(console.error);