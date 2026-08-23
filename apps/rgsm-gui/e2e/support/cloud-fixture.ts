import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  CHILD_SAVE_BYTES,
  CHILD_SNAPSHOT_ID,
  DEVICE_A_ID,
  DEVICE_A_NAME,
  DEVICE_B_ID,
  DEVICE_B_NAME,
  GAME_NAME,
  PARENT_SAVE_BYTES,
  PARENT_SNAPSHOT_ID,
  STORAGE_KEY,
} from './constants';

const fixtureRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../fixtures/legacy-cloud-v1');

export type DeviceLayout = {
  id: string;
  name: string;
  appDataDir: string;
  savePath: string;
  archiveRoot: string;
};

export type SeededCloud = {
  cloudRoot: string;
  deviceA: DeviceLayout;
  deviceB: DeviceLayout;
  v1ConfigBytes: Buffer;
  v1BackupsBytes: Buffer;
  parentArchiveBytes: Buffer;
  childArchiveBytes: Buffer;
};

export function deviceLayout(runRoot: string, device: 'a' | 'b'): DeviceLayout {
  const id = device === 'a' ? DEVICE_A_ID : DEVICE_B_ID;
  const name = device === 'a' ? DEVICE_A_NAME : DEVICE_B_NAME;
  return {
    id,
    name,
    appDataDir: join(runRoot, `app-data-${device}`),
    savePath: join(runRoot, `saves-${device}`, SAVE_RELATIVE),
    archiveRoot: join(runRoot, `app-data-${device}`, 'save_data'),
  };
}

const SAVE_RELATIVE = join('Echo Keep', 'progress.txt');

function applyPlaceholders(template: string, replacements: Record<string, string>): string {
  let next = template;
  for (const [token, value] of Object.entries(replacements)) {
    next = next.split(token).join(value);
  }
  return next;
}

async function writeText(path: string, content: string): Promise<void> {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, content, 'utf8');
}

async function copyArchive(name: string, destination: string): Promise<Buffer> {
  const source = join(fixtureRoot, 'archives', name);
  await mkdir(dirname(destination), { recursive: true });
  await copyFile(source, destination);
  return readFile(destination);
}

export async function seedLegacyV1Scene(runRoot: string): Promise<SeededCloud> {
  const cloudRoot = join(runRoot, 'cloud');
  const deviceA = deviceLayout(runRoot, 'a');
  const deviceB = deviceLayout(runRoot, 'b');
  const replacements = {
    __BACKUP_PATH__: 'save_data',
    __CLOUD_ROOT__: cloudRoot.replaceAll('\\', '/'),
    __SAVE_PATH_A__: deviceA.savePath.replaceAll('\\', '/'),
    __SAVE_PATH_B__: deviceB.savePath.replaceAll('\\', '/'),
  };

  const configTemplate = await readFile(join(fixtureRoot, 'GameSaveManager.config.json'), 'utf8');
  const backupsTemplate = await readFile(join(fixtureRoot, 'Backups.json'), 'utf8');
  const configJson = applyPlaceholders(configTemplate, replacements);
  const backupsJson = backupsTemplate;

  const cloudConfigPath = join(cloudRoot, 'GameSaveManager.config.json');
  const cloudBackupsPath = join(cloudRoot, 'save_data', STORAGE_KEY, 'Backups.json');
  await writeText(cloudConfigPath, configJson);
  await writeText(cloudBackupsPath, backupsJson);

  const parentArchiveBytes = await copyArchive(
    `${PARENT_SNAPSHOT_ID}.zip`,
    join(cloudRoot, 'save_data', STORAGE_KEY, `${PARENT_SNAPSHOT_ID}.zip`)
  );
  const childArchiveBytes = await copyArchive(
    `${CHILD_SNAPSHOT_ID}.zip`,
    join(cloudRoot, 'save_data', STORAGE_KEY, `${CHILD_SNAPSHOT_ID}.zip`)
  );

  for (const device of [deviceA, deviceB]) {
    const localConfig = applyPlaceholders(configTemplate, {
      ...replacements,
      __BACKUP_PATH__: device.archiveRoot.replaceAll('\\', '/'),
    });
    await writeText(join(device.appDataDir, 'GameSaveManager.config.json'), localConfig);
    await writeText(join(device.appDataDir, 'save_data', STORAGE_KEY, 'Backups.json'), backupsJson);
    await copyArchive(
      `${PARENT_SNAPSHOT_ID}.zip`,
      join(device.appDataDir, 'save_data', STORAGE_KEY, `${PARENT_SNAPSHOT_ID}.zip`)
    );
    await copyArchive(
      `${CHILD_SNAPSHOT_ID}.zip`,
      join(device.appDataDir, 'save_data', STORAGE_KEY, `${CHILD_SNAPSHOT_ID}.zip`)
    );
    await writeText(device.savePath, CHILD_SAVE_BYTES);
  }

  return {
    cloudRoot,
    deviceA,
    deviceB,
    v1ConfigBytes: Buffer.from(configJson, 'utf8'),
    v1BackupsBytes: Buffer.from(backupsJson, 'utf8'),
    parentArchiveBytes,
    childArchiveBytes,
  };
}

export async function seedEmptyCloudWithLocalGame(
  runRoot: string,
  options: { gameOnA?: boolean; gameOnB?: boolean } = {}
): Promise<{
  cloudRoot: string;
  deviceA: DeviceLayout;
  deviceB: DeviceLayout;
}> {
  const gameOnA = options.gameOnA ?? true;
  const gameOnB = options.gameOnB ?? true;
  const cloudRoot = join(runRoot, 'cloud');
  const deviceA = deviceLayout(runRoot, 'a');
  const deviceB = deviceLayout(runRoot, 'b');
  const configTemplate = await readFile(join(fixtureRoot, 'GameSaveManager.config.json'), 'utf8');
  await mkdir(cloudRoot, { recursive: true });
  for (const device of [deviceA, deviceB]) {
    let localConfig = applyPlaceholders(configTemplate, {
      __BACKUP_PATH__: device.archiveRoot.replaceAll('\\', '/'),
      __CLOUD_ROOT__: cloudRoot.replaceAll('\\', '/'),
      __SAVE_PATH_A__: deviceA.savePath.replaceAll('\\', '/'),
      __SAVE_PATH_B__: deviceB.savePath.replaceAll('\\', '/'),
    });
    const hasGame = device === deviceA ? gameOnA : gameOnB;
    if (!hasGame) {
      const parsed = JSON.parse(localConfig) as { games: unknown[] };
      parsed.games = [];
      localConfig = `${JSON.stringify(parsed, null, 2)}\n`;
    }
    await writeText(join(device.appDataDir, 'GameSaveManager.config.json'), localConfig);
    if (hasGame) {
      await writeText(
        join(device.appDataDir, 'save_data', STORAGE_KEY, 'Backups.json'),
        `${JSON.stringify({ name: GAME_NAME, backups: [], device_heads: {}, sync_version: 0 }, null, 2)}\n`
      );
    }
    await writeText(device.savePath, CHILD_SAVE_BYTES);
  }
  return { cloudRoot, deviceA, deviceB };
}

/// Seed local snapshots marked as automatic (Timer) before the host starts, so
/// the first library activation publishes them with automatic provenance. V2
/// forbids re-marking provenance through the API, and the HTTP host has no
/// auto-backup timer, so fixture data is the only way to get Timer snapshots.
export async function seedLocalAutomaticSnapshots(
  device: DeviceLayout,
  describes: string[]
): Promise<string[]> {
  const ids = describes.map((_, index) => `2026-01-01_00-00-0${index + 1}`);
  const dir = join(device.archiveRoot, STORAGE_KEY);
  await mkdir(dir, { recursive: true });
  const backups = [];
  for (const [index, describe] of describes.entries()) {
    const date = ids[index]!;
    const archivePath = join(dir, `${date}.7z`);
    const bytes = Buffer.from(`automatic archive ${describe}\n`, 'utf8');
    await writeFile(archivePath, bytes);
    backups.push({
      date,
      describe,
      path: archivePath,
      archive_format: 'seven_z',
      size: bytes.length,
      parent: index === 0 ? null : ids[index - 1],
      device_id: device.id,
      created_by: 'Timer',
    });
  }
  await writeText(
    join(dir, 'Backups.json'),
    `${JSON.stringify(
      {
        name: GAME_NAME,
        backups,
        device_heads: { [device.id]: ids.at(-1) },
        sync_version: 0,
      },
      null,
      2
    )}\n`
  );
  return ids;
}

export async function writeSave(device: DeviceLayout, contents: string): Promise<void> {
  await writeText(device.savePath, contents);
}

export async function readSave(device: DeviceLayout): Promise<string> {
  return readFile(device.savePath, 'utf8');
}

export { PARENT_SAVE_BYTES, CHILD_SAVE_BYTES, GAME_NAME };
