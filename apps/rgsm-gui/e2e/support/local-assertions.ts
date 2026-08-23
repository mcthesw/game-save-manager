import { existsSync } from 'node:fs';
import { readdir, readFile, stat, utimes } from 'node:fs/promises';
import { basename, join, relative, sep } from 'node:path';
import { expect } from '@playwright/test';
import { STORAGE_KEY } from './constants';

export type BackupsEntry = {
  date: string;
  describe: string;
  path: string;
  archive_format?: string;
  size?: number;
  parent?: string | null;
  device_id?: string | null;
  created_by?: string;
  archive_hash?: string | null;
};

export type BackupsFile = {
  name: string;
  backups: BackupsEntry[];
  device_heads?: Record<string, string>;
};

export function localSnapshotsDir(appDataDir: string, storageKey: string = STORAGE_KEY): string {
  return join(appDataDir, 'save_data', storageKey);
}

export async function readBackupsJson(
  appDataDir: string,
  storageKey: string = STORAGE_KEY
): Promise<BackupsFile> {
  const path = join(localSnapshotsDir(appDataDir, storageKey), 'Backups.json');
  expect(existsSync(path), `missing ${path}`).toBe(true);
  return JSON.parse(await readFile(path, 'utf8')) as BackupsFile;
}

export async function snapshotMeta(
  appDataDir: string,
  snapshotId: string,
  storageKey: string = STORAGE_KEY
): Promise<BackupsEntry> {
  const file = await readBackupsJson(appDataDir, storageKey);
  const entry = file.backups.find((item) => item.date === snapshotId);
  expect(entry, `missing snapshot ${snapshotId} in Backups.json`).toBeTruthy();
  return entry!;
}

export async function expectSnapshotDates(
  appDataDir: string,
  expectedDates: string[],
  storageKey: string = STORAGE_KEY
): Promise<void> {
  await expect
    .poll(
      async () => {
        if (!existsSync(join(localSnapshotsDir(appDataDir, storageKey), 'Backups.json'))) {
          return [];
        }
        const file = await readBackupsJson(appDataDir, storageKey);
        return file.backups.map((item) => item.date).sort();
      },
      { timeout: 15_000 }
    )
    .toEqual([...expectedDates].sort());
}

export async function expectSnapshotParent(
  appDataDir: string,
  snapshotId: string,
  parentId: string | null,
  storageKey: string = STORAGE_KEY
): Promise<void> {
  await expect
    .poll(
      async () => {
        const file = await readBackupsJson(appDataDir, storageKey);
        return file.backups.find((item) => item.date === snapshotId)?.parent ?? null;
      },
      { timeout: 15_000 }
    )
    .toBe(parentId);
}

export async function expectLocalHead(
  appDataDir: string,
  deviceId: string,
  snapshotId: string | null,
  storageKey: string = STORAGE_KEY
): Promise<void> {
  await expect
    .poll(
      async () => {
        const file = await readBackupsJson(appDataDir, storageKey);
        return file.device_heads?.[deviceId] ?? null;
      },
      { timeout: 15_000 }
    )
    .toBe(snapshotId);
}

export function localArchiveExists(
  appDataDir: string,
  snapshotId: string,
  storageKey: string = STORAGE_KEY
): boolean {
  return existsSync(join(localSnapshotsDir(appDataDir, storageKey), `${snapshotId}.7z`));
}

export function extraBackupDir(appDataDir: string, storageKey: string = STORAGE_KEY): string {
  return join(localSnapshotsDir(appDataDir, storageKey), 'extra_backup');
}

/** Extra backup archive stems (filename without extension), newest first not guaranteed. */
export async function listExtraBackupDates(
  appDataDir: string,
  storageKey: string = STORAGE_KEY
): Promise<string[]> {
  const dir = extraBackupDir(appDataDir, storageKey);
  if (!existsSync(dir)) return [];
  const files = await readdir(dir);
  return files
    .filter((name) => name.endsWith('.7z') || name.endsWith('.zip'))
    .map((name) => name.replace(/\.(7z|zip)$/, ''));
}

export async function setMtimeMs(path: string, ms: number): Promise<void> {
  const date = new Date(ms);
  await utimes(path, date, date);
}

export async function mtimeMs(path: string): Promise<number> {
  return (await stat(path)).mtimeMs;
}

export type TreeEntry = { kind: 'file' | 'dir'; mtimeMs: number };

/** Recursively lists a directory tree: relative path (posix separators) → entry. */
export async function listTree(root: string): Promise<Map<string, TreeEntry>> {
  const result = new Map<string, TreeEntry>();
  async function walk(dir: string): Promise<void> {
    for (const name of await readdir(dir)) {
      const full = join(dir, name);
      const info = await stat(full);
      const rel = relative(root, full).split(sep).join('/');
      result.set(rel, {
        kind: info.isDirectory() ? 'dir' : 'file',
        mtimeMs: info.mtimeMs,
      });
      if (info.isDirectory()) await walk(full);
    }
  }
  await walk(root);
  return result;
}

/**
 * Asserts every expected relative path exists with the recorded mtime.
 * `toleranceMs` absorbs filesystem timestamp granularity (7z NT time is
 * precise; some filesystems truncate to seconds).
 */
export async function expectTreeMtimes(
  root: string,
  expected: Map<string, TreeEntry>,
  toleranceMs = 2000
): Promise<void> {
  const actual = await listTree(root);
  for (const [rel, entry] of expected) {
    const found = actual.get(rel);
    expect(found, `missing ${rel} under ${root}`).toBeTruthy();
    expect(found!.kind).toBe(entry.kind);
    expect(
      Math.abs(found!.mtimeMs - entry.mtimeMs),
      `${rel} mtime drifted: expected ${entry.mtimeMs}, got ${found!.mtimeMs}`
    ).toBeLessThanOrEqual(toleranceMs);
  }
}

/** Reads Backups.json `path` field basename checks are brittle; use archive existence instead. */
export function archiveFileName(snapshotId: string): string {
  return `${snapshotId}.7z`;
}

export function extraBackupPath(appDataDir: string, date: string, storageKey?: string): string {
  return join(extraBackupDir(appDataDir, storageKey), `${date}.7z`);
}

export { basename };
