import { mkdir, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));

export function localCloudRoot(repoRoot = path.resolve(scriptDir, '..')) {
  return path.join(repoRoot, 'tmp', 'local-cloud');
}

function sharedGame(id, name) {
  return {
    name,
    storage_key: id,
    save_units: [
      {
        id: 1,
        source: {
          type: 'concrete',
          unit_type: 'Folder',
        },
      },
    ],
    next_save_unit_id: 2,
    ludusavi_meta: null,
  };
}

function emptyGameManifest(gameId) {
  return {
    game_id: gameId,
    snapshots: {},
    device_heads: {},
    local_archives: {},
  };
}

export function seedDocuments() {
  return {
    'v2/namespace.json': {
      schema_version: 2,
    },
    'v2/shared-library.json': {
      schema_version: 2,
      games: [sharedGame('seed-alpha', 'Seed Alpha'), sharedGame('seed-beta', 'Seed Beta')],
    },
    'v2/cloud-manifest.json': {
      schema_version: 2,
      revision: 0,
      games: {
        'seed-alpha': emptyGameManifest('seed-alpha'),
        'seed-beta': emptyGameManifest('seed-beta'),
      },
    },
    'v2/deletions.json': {
      schema_version: 1,
      revision: 0,
      deleted_profiles: {},
      deleted_games: {},
    },
  };
}

export async function resetLocalCloud(root = localCloudRoot()) {
  await rm(root, { recursive: true, force: true });
  await mkdir(root, { recursive: true });
  return root;
}

export async function emptyLocalCloud(root = localCloudRoot()) {
  return resetLocalCloud(root);
}

export async function seedLocalCloud(root = localCloudRoot()) {
  await resetLocalCloud(root);
  for (const [relativePath, document] of Object.entries(seedDocuments())) {
    const filePath = path.join(root, relativePath);
    await mkdir(path.dirname(filePath), { recursive: true });
    await writeFile(filePath, `${JSON.stringify(document, null, 2)}\n`, 'utf8');
  }
  return root;
}

function printUsage(root) {
  console.log(`Local cloud root:\n  ${root}\n`);
  console.log('Commands: reset | empty | seed');
  console.log('Connect with backend "Local folder" and this absolute path.');
}

async function main(command = process.argv[2]) {
  const root = localCloudRoot();
  switch (command) {
    case 'reset':
      await resetLocalCloud(root);
      break;
    case 'empty':
      await emptyLocalCloud(root);
      break;
    case 'seed':
      await seedLocalCloud(root);
      break;
    default:
      printUsage(root);
      process.exitCode = command ? 1 : 0;
      return;
  }
  printUsage(root);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
