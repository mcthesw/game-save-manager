import AdmZip from 'adm-zip';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { DEVICE_A_ID, GAME_NAME, PARENT_SNAPSHOT_ID, CHILD_SNAPSHOT_ID } from './constants';
import { workspacePath } from './rgsm-instance';

export async function seedReleasedUpgrade(runRoot: string, version: '1.7.0' | '1.8.0') {
  const config = JSON.parse(
    await readFile(
      workspacePath(
        'crates',
        'rgsm-core',
        'tests',
        'fixtures',
        'config-upgrade',
        `config_v${version.replaceAll('.', '_')}.json`
      ),
      'utf8'
    )
  );
  const appDataDir = join(runRoot, 'app-data');
  const archiveDir = join(appDataDir, 'save_data', GAME_NAME);
  const gameRoot = join(runRoot, 'Games[Main]');
  const units = [
    { name: 'Saves', type: 'Folder', replace: false },
    { name: '[prefs].json', type: 'File', replace: true },
    { name: 'Replaced', type: 'Folder', replace: true },
  ];
  const ids = version === '1.8.0' ? [7, 11, 12] : [0, 1, 2];
  config.backup_path = join(appDataDir, 'save_data').replaceAll('\\', '/');
  config.games[0].name = GAME_NAME;
  config.games[0].save_paths = units.map((unit, index) => ({
    ...(version === '1.8.0' ? { id: ids[index] } : {}),
    unit_type: unit.type,
    paths: { [DEVICE_A_ID]: join(gameRoot, unit.name).replaceAll('\\', '/') },
    delete_before_apply: unit.replace,
  }));
  config.games[0].game_paths = { [DEVICE_A_ID]: join(gameRoot, 'game.exe').replaceAll('\\', '/') };
  if (version === '1.8.0') config.games[0].next_save_unit_id = 13;
  // Isolate the historical configuration before startup, without supplying
  // roots/bindings or changing restoration and extra-backup defaults.
  config.devices = { [DEVICE_A_ID]: { id: DEVICE_A_ID, name: 'Upgrade device' } };
  config.quick_action.enable_sound = false;
  config.quick_action.enable_notification = false;
  await mkdir(archiveDir, { recursive: true });
  await writeFile(join(appDataDir, 'GameSaveManager.config.json'), JSON.stringify(config));
  const savePaths: string[] = [];
  for (const unit of units) {
    const path = join(gameRoot, unit.name);
    await mkdir(unit.type === 'Folder' ? path : gameRoot, { recursive: true });
    const file = unit.type === 'Folder' ? join(path, 'profile.sav') : path;
    savePaths.push(file);
    await writeFile(file, 'live-before-upgrade');
    if (unit.type === 'Folder') await writeFile(join(path, 'new-only.txt'), 'keep-unless-replaced');
  }
  const backups = [];
  const originalArchives = new Map<string, Buffer>();
  for (const date of [PARENT_SNAPSHOT_ID, CHILD_SNAPSHOT_ID]) {
    const zip = new AdmZip();
    if (version === '1.8.0') {
      zip.addZipComment('RGSM_ARCHIVE_V2\n{"version":2,"compression":"zstd:3"}');
    }
    for (const [index, unit] of units.entries()) {
      const entry = `${version === '1.8.0' ? `${ids[index]}/` : ''}${unit.name}${unit.type === 'Folder' ? '/profile.sav' : ''}`;
      zip.addFile(entry, Buffer.from(`saved-${date}`));
    }
    const bytes = zip.toBuffer();
    const path = join(archiveDir, `${date}.zip`);
    await writeFile(path, bytes);
    originalArchives.set(path, bytes);
    backups.push({
      date,
      describe: `Original ${date}`,
      path,
      size: bytes.length,
      parent: date === CHILD_SNAPSHOT_ID ? PARENT_SNAPSHOT_ID : null,
      ...(version === '1.8.0' ? { device_id: DEVICE_A_ID } : {}),
    });
  }
  await writeFile(
    join(archiveDir, 'Backups.json'),
    JSON.stringify({
      name: GAME_NAME,
      backups,
      ...(version === '1.8.0'
        ? { device_heads: { [DEVICE_A_ID]: PARENT_SNAPSHOT_ID }, sync_version: 3 }
        : { head: PARENT_SNAPSHOT_ID }),
    })
  );
  return { appDataDir, archiveDir, gameRoot, savePaths, originalArchives, ids };
}
