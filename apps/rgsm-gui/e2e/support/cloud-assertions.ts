import { spawnSync } from 'node:child_process';
import { existsSync, readdirSync } from 'node:fs';
import { readFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { expect } from '@playwright/test';
import {
  CHILD_ARCHIVE_HASH,
  CHILD_ARCHIVE_SIZE,
  CHILD_SNAPSHOT_ID,
  DEVICE_A_ID,
  DEVICE_B_ID,
  PARENT_ARCHIVE_HASH,
  PARENT_ARCHIVE_SIZE,
  PARENT_SNAPSHOT_ID,
  STORAGE_KEY,
  deviceProfileFileName,
} from './constants';
import { xxh3HelperPath } from './rgsm-instance';

export type JsonObject = Record<string, unknown>;

export async function readJson(path: string): Promise<JsonObject> {
  return JSON.parse(await readFile(path, 'utf8')) as JsonObject;
}

export async function readOptionalJson(path: string): Promise<JsonObject | undefined> {
  if (!existsSync(path)) return undefined;
  return readJson(path);
}

export function cloudPaths(cloudRoot: string) {
  return {
    v1Config: join(cloudRoot, 'GameSaveManager.config.json'),
    v1Backups: join(cloudRoot, 'save_data', STORAGE_KEY, 'Backups.json'),
    v1ParentArchive: join(cloudRoot, 'save_data', STORAGE_KEY, `${PARENT_SNAPSHOT_ID}.zip`),
    v1ChildArchive: join(cloudRoot, 'save_data', STORAGE_KEY, `${CHILD_SNAPSHOT_ID}.zip`),
    namespace: join(cloudRoot, 'v2', 'namespace.json'),
    sharedLibrary: join(cloudRoot, 'v2', 'shared-library.json'),
    manifest: join(cloudRoot, 'v2', 'cloud-manifest.json'),
    deletions: join(cloudRoot, 'v2', 'deletions.json'),
    profiles: join(cloudRoot, 'v2', 'device-profiles'),
    v2ParentArchive: join(cloudRoot, 'v2', 'archives', STORAGE_KEY, `${PARENT_SNAPSHOT_ID}.zip`),
    v2ChildArchive: join(cloudRoot, 'v2', 'archives', STORAGE_KEY, `${CHILD_SNAPSHOT_ID}.zip`),
  };
}

export function localOwnerPaths(appDataDir: string, deviceId: string) {
  return {
    ownerRoot: join(appDataDir, 'GameSaveManager.config.v2'),
    localState: join(appDataDir, 'GameSaveManager.config.v2', 'local-state.json'),
    sharedLibrary: join(appDataDir, 'GameSaveManager.config.v2', 'shared-library.json'),
    profile: join(
      appDataDir,
      'GameSaveManager.config.v2',
      'device-profiles',
      deviceProfileFileName(deviceId)
    ),
  };
}

export function localArchivePath(appDataDir: string, snapshotId: string): string {
  return join(appDataDir, 'save_data', STORAGE_KEY, `${snapshotId}.zip`);
}

export async function xxh3File(
  path: string
): Promise<{ size: number; hash: string; bytes: Buffer }> {
  const bytes = await readFile(path);
  const result = spawnSync(xxh3HelperPath(), [path], { encoding: 'utf8' });
  expect(result.status, `xxh3 helper failed for ${path}: ${result.stderr}`).toBe(0);
  const parts = result.stdout.trim().split(/\s+/);
  expect(parts.length).toBeGreaterThanOrEqual(3);
  return { size: Number(parts[1]), hash: parts[2], bytes };
}
export async function expectFileBytes(path: string, expected: Buffer): Promise<void> {
  expect(existsSync(path), `missing ${path}`).toBe(true);
  expect(await readFile(path)).toEqual(expected);
}

export async function expectV1ObjectsUnchanged(
  cloudRoot: string,
  expected: { config: Buffer; backups: Buffer; parent: Buffer; child: Buffer }
): Promise<void> {
  const paths = cloudPaths(cloudRoot);
  await expectFileBytes(paths.v1Config, expected.config);
  await expectFileBytes(paths.v1Backups, expected.backups);
  await expectFileBytes(paths.v1ParentArchive, expected.parent);
  await expectFileBytes(paths.v1ChildArchive, expected.child);
}

export function expectNamespaceDescriptor(value: JsonObject): void {
  expect(value.schema_version).toBe(2);
}

export function expectSharedLibraryHasGame(value: JsonObject): void {
  const games = value.games as Array<{ storage_key?: string; name?: string }>;
  expect(games).toEqual(
    expect.arrayContaining([
      expect.objectContaining({ storage_key: STORAGE_KEY, name: 'Echo Keep' }),
    ])
  );
}

export function gameManifest(manifest: JsonObject) {
  const games = manifest.games as Record<string, JsonObject>;
  expect(games[STORAGE_KEY], 'manifest missing Echo Keep').toBeTruthy();
  return games[STORAGE_KEY];
}

export function snapshotNode(manifest: JsonObject, snapshotId: string) {
  const game = gameManifest(manifest);
  const snapshots = game.snapshots as Record<string, JsonObject>;
  expect(snapshots[snapshotId], `missing snapshot ${snapshotId}`).toBeTruthy();
  return snapshots[snapshotId];
}

export function expectLiveParentChildGraph(manifest: JsonObject): void {
  expect(manifest.schema_version).toBe(2);
  const parent = snapshotNode(manifest, PARENT_SNAPSHOT_ID);
  const child = snapshotNode(manifest, CHILD_SNAPSHOT_ID);
  expect(parent.parent).toBeNull();
  expect(child.parent).toBe(PARENT_SNAPSHOT_ID);
  expect((parent.state as JsonObject).type).toBe('live');
  expect((child.state as JsonObject).type).toBe('live');
}

export function expectDeviceHead(manifest: JsonObject, deviceId: string, snapshotId: string): void {
  const game = gameManifest(manifest);
  const heads = game.device_heads as Record<string, string>;
  expect(heads[deviceId]).toBe(snapshotId);
}

export function expectNoDeviceHead(
  manifest: JsonObject,
  deviceId: string,
  snapshotId: string
): void {
  const game = gameManifest(manifest);
  const heads = game.device_heads as Record<string, string>;
  expect(heads[deviceId]).not.toBe(snapshotId);
}

export function expectFinalTombstone(manifest: JsonObject, snapshotId: string): void {
  const node = snapshotNode(manifest, snapshotId);
  expect((node.state as JsonObject).type).toBe('final_tombstone');
}

export async function expectVerifiedArchives(
  cloudRoot: string,
  expected: { parent: Buffer; child: Buffer }
): Promise<void> {
  const paths = cloudPaths(cloudRoot);
  const parent = await xxh3File(paths.v2ParentArchive);
  const child = await xxh3File(paths.v2ChildArchive);
  expect(parent.bytes).toEqual(expected.parent);
  expect(child.bytes).toEqual(expected.child);
  expect(parent.size).toBe(PARENT_ARCHIVE_SIZE);
  expect(child.size).toBe(CHILD_ARCHIVE_SIZE);
  expect(parent.hash).toBe(PARENT_ARCHIVE_HASH);
  expect(child.hash).toBe(CHILD_ARCHIVE_HASH);
}

export async function expectDeviceProfiles(cloudRoot: string): Promise<void> {
  const files = await readdir(cloudPaths(cloudRoot).profiles);
  expect(files.sort()).toEqual(
    [deviceProfileFileName(DEVICE_A_ID), deviceProfileFileName(DEVICE_B_ID)].sort()
  );
}

export async function readDeviceProfile(cloudRoot: string, deviceId: string): Promise<JsonObject> {
  return readJson(join(cloudPaths(cloudRoot).profiles, deviceProfileFileName(deviceId)));
}

export async function expectLocalGeneration(
  appDataDir: string,
  generation: 'legacy_v1' | 'v2'
): Promise<void> {
  const localState = await readJson(localOwnerPaths(appDataDir, DEVICE_A_ID).localState);
  expect(localState.cloud_namespace_generation).toBe(generation === 'v2' ? 'v2' : 'legacy_v1');
}

export function cutoverProgressPath(appDataDir: string): string | undefined {
  if (!existsSync(appDataDir)) return undefined;
  return readdirSync(appDataDir)
    .filter((name) => name.startsWith('GameSaveManager.cloud-cutover.') && name.endsWith('.json'))
    .map((name) => join(appDataDir, name))[0];
}

export async function expectPersistedCutoverProgress(
  appDataDir: string,
  completedArchives: number
): Promise<void> {
  const path = cutoverProgressPath(appDataDir);
  expect(path, 'cutover progress file missing').toBeTruthy();
  const plan = await readJson(path!);
  const games = plan.games as Array<{ results?: Record<string, unknown> }>;
  const completed = games.reduce(
    (total, game) => total + Object.keys(game.results ?? {}).length,
    0
  );
  expect(completed).toBe(completedArchives);
}

export function expectNoNamespace(cloudRoot: string): void {
  expect(existsSync(cloudPaths(cloudRoot).namespace)).toBe(false);
}

export function expectNoCutoverProgress(appDataDir: string): void {
  expect(cutoverProgressPath(appDataDir)).toBeUndefined();
}
