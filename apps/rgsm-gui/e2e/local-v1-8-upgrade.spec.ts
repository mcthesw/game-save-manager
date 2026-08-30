import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import AdmZip from 'adm-zip';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { DEVICE_A_ID, GAME_NAME } from './support/constants';
import { openApp, openGame, snapshotRow } from './support/gui';
import { listSnapshotsFor } from './support/local-gui';
import {
  createRunRoot,
  hostPost,
  newDeviceContext,
  removeRunRoot,
  startRgsmHost,
  startTestWeb,
  workspacePath,
  type RgsmHost,
} from './support/rgsm-instance';

const RELEASED_DATE = '2025-01-02_03-04-05';
const RELEASED_CONTENT = 'released-save-content';
const RELEASED_REGISTRY_NAMED_FILE_CONTENT = 'ordinary-file-named-registry';

type MigratedConfig = {
  version: string;
  games: Array<{
    name: string;
    save_paths: Array<{
      id: number;
      source: {
        type: string;
        pattern?: string;
        expected_type?: string;
        unit_type?: string;
        paths?: Record<string, string>;
      };
    }>;
    device_bindings?: Record<string, { rootIds?: number[] }>;
  }>;
  settings: Record<string, unknown>;
  devices: Record<
    string,
    { resources: Array<{ id: number; kind: { type: string; path?: string } }> }
  >;
  [key: string]: unknown;
};

function normalizePath(path: string): string {
  return path.replaceAll('\\', '/').replace(/\/$/, '').toLowerCase();
}

async function seedV1_8Scene(runRoot: string) {
  const appDataDir = join(runRoot, 'app-data');
  const archiveRoot = join(appDataDir, 'save_data');
  const gameRoot = join(runRoot, 'game-root[legacy]');
  const savePath = join(gameRoot, 'Saved', 'profile.sav');
  const registryNamedFilePath = join(gameRoot, 'registry.reg');
  const logPath = join(runRoot, 'host.log');
  const configPath = workspacePath(
    'crates',
    'rgsm-core',
    'tests',
    'fixtures',
    'config-upgrade',
    'config_v1_8_0.json'
  );
  const config = JSON.parse(await readFile(configPath, 'utf8')) as Record<string, any>;
  config.backup_path = archiveRoot.replaceAll('\\', '/');
  config.games[0].name = GAME_NAME;
  config.games[0].save_paths = [
    {
      id: 11,
      unit_type: 'Folder',
      paths: { [DEVICE_A_ID]: '<root>/Saved' },
      delete_before_apply: true,
    },
    {
      id: 12,
      unit_type: 'File',
      paths: { [DEVICE_A_ID]: registryNamedFilePath.replaceAll('\\', '/') },
      delete_before_apply: true,
    },
  ];
  config.games[0].game_paths = {
    [DEVICE_A_ID]: join(gameRoot, 'game.exe').replaceAll('\\', '/'),
  };
  config.games[0].next_save_unit_id = 13;
  config.settings.extra_backup_when_apply = false;
  config.settings.confirm_before_apply_snapshot = false;
  config.devices = {
    [DEVICE_A_ID]: {
      id: DEVICE_A_ID,
      name: 'Upgrade device',
      game_roots: [gameRoot.replaceAll('\\', '/')],
    },
  };

  await mkdir(appDataDir, { recursive: true });
  await writeFile(
    join(appDataDir, 'GameSaveManager.config.json'),
    `${JSON.stringify(config, null, 2)}\n`,
    'utf8'
  );
  await mkdir(dirname(savePath), { recursive: true });
  await writeFile(savePath, 'current-save', 'utf8');
  await writeFile(registryNamedFilePath, 'current-ordinary-file', 'utf8');

  const snapshotsDir = join(archiveRoot, GAME_NAME);
  const releasedArchive = join(snapshotsDir, `${RELEASED_DATE}.zip`);
  const zip = new AdmZip();
  zip.addFile('11/Saved/profile.sav', Buffer.from(RELEASED_CONTENT, 'utf8'));
  zip.addFile('12/registry.reg', Buffer.from(RELEASED_REGISTRY_NAMED_FILE_CONTENT, 'utf8'));
  zip.addZipComment('RGSM_ARCHIVE_V2\n{"version":2,"compression":"zstd:3"}');
  await mkdir(snapshotsDir, { recursive: true });
  await writeFile(releasedArchive, zip.toBuffer());
  await writeFile(
    join(snapshotsDir, 'Backups.json'),
    `${JSON.stringify(
      {
        name: GAME_NAME,
        backups: [
          {
            date: RELEASED_DATE,
            describe: 'Released 1.8 snapshot',
            path: `save_data/${GAME_NAME}/${RELEASED_DATE}.zip`,
            size: (await readFile(releasedArchive)).length,
            device_id: DEVICE_A_ID,
          },
        ],
        device_heads: { [DEVICE_A_ID]: RELEASED_DATE },
        sync_version: 3,
        last_sync_device: DEVICE_A_ID,
      },
      null,
      2
    )}\n`,
    'utf8'
  );

  return {
    appDataDir,
    archiveRoot,
    gameRoot,
    savePath,
    registryNamedFilePath,
    logPath,
    releasedArchive,
  };
}

async function getMigratedConfig(host: RgsmHost, gameRoot: string): Promise<MigratedConfig> {
  const result = await hostPost<MigratedConfig>(host, '/api/v1/get-local-config');
  expect(result.ok, result.raw).toBe(true);
  const config = result.data;
  const root = config.devices[DEVICE_A_ID].resources.find(
    (resource) =>
      resource.kind.type === 'gameRoot' &&
      resource.kind.path !== undefined &&
      normalizePath(resource.kind.path) === normalizePath(gameRoot)
  );
  expect(root, 'the migrated 1.8 game root was not retained').toBeTruthy();
  const game = config.games.find((candidate) => candidate.name === GAME_NAME);
  expect(game).toBeTruthy();
  expect(game!.device_bindings?.[DEVICE_A_ID]?.rootIds).toEqual([root!.id]);
  return config;
}

async function applyAndExpectSave(
  page: Page,
  snapshotId: string,
  expectations: Array<{ path: string; content: string }>
): Promise<void> {
  await snapshotRow(page, snapshotId).getByRole('button', { name: 'Apply' }).click();
  await expect
    .poll(
      async () =>
        Promise.all(
          expectations.map(async ({ path }) => {
            try {
              return await readFile(path, 'utf8');
            } catch {
              // delete_before_apply intentionally leaves a short missing-target window.
              return null;
            }
          })
        ),
      { timeout: 30_000 }
    )
    .toEqual(expectations.map(({ content }) => content));
}

test('1.8 dynamic and concrete save paths keep their V2 zip usable after the 1.9 upgrade', async ({
  browser,
}) => {
  test.skip(process.platform !== 'win32', 'the released 1.8 upgrade contract targets Windows');

  const runRoot = await createRunRoot('local-v1-8-upgrade');
  const scene = await seedV1_8Scene(runRoot);
  const originalArchive = await readFile(scene.releasedArchive);
  const web = await startTestWeb();
  let host: RgsmHost | undefined;
  let context: BrowserContext | undefined;
  try {
    host = await startRgsmHost({
      appDataDir: scene.appDataDir,
      deviceId: DEVICE_A_ID,
      logPath: scene.logPath,
    });
    const migrated = await getMigratedConfig(host, scene.gameRoot);
    expect(migrated.version).toBe('1.9.0');
    expect(migrated.games[0].save_paths).toEqual([
      expect.objectContaining({
        id: 11,
        source: expect.objectContaining({
          type: 'manifestPattern',
          expected_type: 'Folder',
          pattern: '<root>/Saved',
        }),
      }),
      expect.objectContaining({
        id: 12,
        source: expect.objectContaining({
          type: 'concrete',
          unit_type: 'File',
          paths: { [DEVICE_A_ID]: scene.registryNamedFilePath.replaceAll('\\', '/') },
        }),
      }),
    ]);

    ({ context } = await newDeviceContext(browser, host));
    let page = context.pages()[0]!;
    await openApp(page);
    await openGame(page);
    await expect(snapshotRow(page, RELEASED_DATE)).toContainText('Released 1.8 snapshot');

    await applyAndExpectSave(page, RELEASED_DATE, [
      { path: scene.savePath, content: RELEASED_CONTENT },
      {
        path: scene.registryNamedFilePath,
        content: RELEASED_REGISTRY_NAMED_FILE_CONTENT,
      },
    ]);

    await writeFile(scene.savePath, 'created-by-1.9', 'utf8');
    await writeFile(scene.registryNamedFilePath, 'ordinary-file-created-by-1.9', 'utf8');
    await page.getByPlaceholder('New backup description').fill('Created after upgrade');
    await page.getByRole('button', { name: 'Create new snapshot' }).click();
    await expect
      .poll(async () => (await listSnapshotsFor(host!, GAME_NAME)).length, { timeout: 30_000 })
      .toBe(2);
    const created = (await listSnapshotsFor(host, GAME_NAME)).find(
      (snapshot) => snapshot.describe === 'Created after upgrade'
    );
    expect(created).toBeTruthy();
    expect(created!.date).not.toBe(RELEASED_DATE);
    await expect(
      readFile(join(scene.archiveRoot, GAME_NAME, `${created!.date}.7z`))
    ).resolves.toBeTruthy();

    await context.close();
    context = undefined;
    await host.stop();
    host = await startRgsmHost({
      appDataDir: scene.appDataDir,
      deviceId: DEVICE_A_ID,
      logPath: scene.logPath,
    });
    ({ context } = await newDeviceContext(browser, host));
    page = context.pages()[0]!;
    await openApp(page);
    await openGame(page);
    await expect(snapshotRow(page, RELEASED_DATE)).toBeVisible();
    await expect(snapshotRow(page, created!.date)).toBeVisible();

    await writeFile(scene.savePath, 'before-old-restore', 'utf8');
    await writeFile(scene.registryNamedFilePath, 'ordinary-file-before-old-restore', 'utf8');
    await applyAndExpectSave(page, RELEASED_DATE, [
      { path: scene.savePath, content: RELEASED_CONTENT },
      {
        path: scene.registryNamedFilePath,
        content: RELEASED_REGISTRY_NAMED_FILE_CONTENT,
      },
    ]);

    await writeFile(scene.savePath, 'before-new-restore', 'utf8');
    await writeFile(scene.registryNamedFilePath, 'ordinary-file-before-new-restore', 'utf8');
    await applyAndExpectSave(page, created!.date, [
      { path: scene.savePath, content: 'created-by-1.9' },
      { path: scene.registryNamedFilePath, content: 'ordinary-file-created-by-1.9' },
    ]);
    expect(await readFile(scene.releasedArchive)).toEqual(originalArchive);
  } finally {
    await context?.close();
    await host?.stop();
    await web.stop();
    await removeRunRoot(runRoot);
  }
});
