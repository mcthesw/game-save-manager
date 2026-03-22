// Modified From https://github.com/zzzgydi/clash-verge/blob/main/scripts/portable.mjs
// GPL-3.0
import fs from 'fs-extra';
import path from 'path';
import AdmZip from 'adm-zip';
import { getOctokit, context } from '@actions/github';

async function resolvePortable() {
  if (process.platform !== 'win32') return;

  const releaseDir = './target/release';

  if (!(await fs.pathExists(releaseDir))) {
    throw new Error('could not found the release dir');
  }

  const zip = new AdmZip();

  zip.addLocalFile(path.join(releaseDir, 'rgsm.exe'));
  // zip.addLocalFolder(path.join(releaseDir, "resources"), "resources");

  const cargoToml = await fs.readFile(
    path.join('./apps/rgsm-gui/src-tauri', 'Cargo.toml'),
    'utf-8',
  );
  const versionMatch = cargoToml.match(/^version\s*=\s*"([^"]+)"/m);
  if (!versionMatch) {
    throw new Error('could not read version from apps/rgsm-gui/src-tauri/Cargo.toml');
  }
  const version = versionMatch[1];

  const zipFile = `RGSM_${version}_x64-portable.zip`;
  zip.writeZip(zipFile);

  console.log('[INFO]: create portable zip successfully');

  if (process.env.GITHUB_TOKEN === undefined) {
    throw new Error('GITHUB_TOKEN is required');
  }

  const options = { owner: context.repo.owner, repo: context.repo.repo };
  const github = getOctokit(process.env.GITHUB_TOKEN);

  console.log('[INFO]: upload to ', process.env.RELEASE_ID);

  // https://octokit.github.io/rest.js
  await github.rest.repos.uploadReleaseAsset({
    ...options,
    release_id: process.env.RELEASE_ID,
    name: zipFile,
    data: zip.toBuffer(),
  });
}

resolvePortable().catch(console.error);
