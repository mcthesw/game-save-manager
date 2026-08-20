import type { CloudArchiveGameView, CloudArchiveSnapshotView } from '../../api/commands';
import { $t } from '../../i18n';

/**
 * Snapshot availability predicates shared by the management page and its
 * snapshot table. All functions are pure: state (cloud view, local catalog,
 * retention set) is passed in explicitly.
 */
export function cloudSnapshotOf(
  cloudGame: CloudArchiveGameView | null,
  date: string
): CloudArchiveSnapshotView | null {
  return cloudGame?.snapshots.find((snapshot) => snapshot.snapshot_id === date) ?? null;
}

export function isSnapshotOnDevice(cloudGame: CloudArchiveGameView | null, date: string) {
  return Boolean(cloudSnapshotOf(cloudGame, date)?.local_verified);
}

export function isSnapshotInCloud(cloudGame: CloudArchiveGameView | null, date: string) {
  return Boolean(cloudSnapshotOf(cloudGame, date)?.cloud_verified);
}

export function canUploadSnapshot(cloudGame: CloudArchiveGameView | null, date: string) {
  if (!cloudGame) return false;
  const snapshot = cloudSnapshotOf(cloudGame, date);
  if (!snapshot) return true;
  return snapshot.local_verified && !snapshot.cloud_verified;
}

export function canDownloadSnapshot(cloudGame: CloudArchiveGameView | null, date: string) {
  const snapshot = cloudSnapshotOf(cloudGame, date);
  return Boolean(snapshot?.cloud_verified && !snapshot.local_verified);
}

export function canEvictSnapshot(cloudGame: CloudArchiveGameView | null, date: string) {
  const snapshot = cloudSnapshotOf(cloudGame, date);
  return Boolean(snapshot?.local_verified);
}

export function canEvictCloudSnapshot(cloudGame: CloudArchiveGameView | null, date: string) {
  return Boolean(cloudSnapshotOf(cloudGame, date)?.cloud_verified);
}

export function canApplySnapshot(
  localCatalogDates: Set<string>,
  cloudGame: CloudArchiveGameView | null,
  date: string
) {
  if (!localCatalogDates.has(date)) return false;
  const snapshot = cloudSnapshotOf(cloudGame, date);
  return !snapshot || snapshot.local_verified;
}

export function isRetentionProtectedDate(
  retentionProtectedDates: Set<string>,
  cloudGame: CloudArchiveGameView | null,
  date: string
) {
  return (
    retentionProtectedDates.has(date) ||
    Boolean(cloudSnapshotOf(cloudGame, date)?.retention_protected)
  );
}

export function snapshotLocationLabel(cloudGame: CloudArchiveGameView | null, date: string) {
  const local = isSnapshotOnDevice(cloudGame, date);
  const cloud = isSnapshotInCloud(cloudGame, date);
  if (local && cloud) return $t('manage.location_both');
  if (local) return $t('manage.location_local');
  if (cloud) return $t('manage.location_cloud');
  const elsewhere = cloudSnapshotOf(cloudGame, date)?.reported_on_devices.length ?? 0;
  if (elsewhere > 0) return $t('sync_settings.archives.available_other_device');
  return $t('manage.location_missing');
}
