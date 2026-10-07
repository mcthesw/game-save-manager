import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { STORAGE_KEY } from './constants';

type ArchiveReference = { archive_name?: string | null; archive_format?: string };

function archivePath(directory: string, id: string, reference?: ArchiveReference): string {
  if (reference) {
    const extension = reference.archive_format === 'seven_z' ? '7z' : 'zip';
    return join(directory, reference.archive_name ?? `${id}.${extension}`);
  }
  // Historical fixtures can precede their catalog; new names come only from metadata.
  const sevenZ = join(directory, `${id}.7z`);
  return existsSync(sevenZ) ? sevenZ : join(directory, `${id}.zip`);
}

export function localArchivePath(
  appDataDir: string,
  snapshotId: string,
  storageKey: string = STORAGE_KEY
): string {
  const directory = join(appDataDir, 'save_data', storageKey);
  const catalogPath = join(directory, 'Backups.json');
  const catalog: { backups: Array<ArchiveReference & { date: string }> } = existsSync(catalogPath)
    ? JSON.parse(readFileSync(catalogPath, 'utf8'))
    : { backups: [] };
  return archivePath(
    directory,
    snapshotId,
    catalog.backups.find((entry) => entry.date === snapshotId)
  );
}

export function cloudArchivePath(cloudRoot: string, snapshotId: string): string {
  const manifestPath = join(cloudRoot, 'v2', 'cloud-manifest.json');
  const manifest: { games: Record<string, { snapshots: Record<string, ArchiveReference> }> } =
    existsSync(manifestPath) ? JSON.parse(readFileSync(manifestPath, 'utf8')) : { games: {} };
  return archivePath(
    join(cloudRoot, 'v2', 'archives', STORAGE_KEY),
    snapshotId,
    manifest.games[STORAGE_KEY]?.snapshots[snapshotId]
  );
}
