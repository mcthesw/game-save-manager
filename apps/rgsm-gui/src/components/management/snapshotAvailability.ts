import type { CloudArchiveGameView, CloudArchiveSnapshotView } from '../../api/commands';

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

export type SnapshotAvailability = {
  onDevice: boolean;
  inCloud: boolean;
  onOtherDevice: boolean;
};

export function snapshotAvailability(
  localCatalogDates: Set<string>,
  cloudGame: CloudArchiveGameView | null,
  date: string
): SnapshotAvailability {
  const snapshot = cloudSnapshotOf(cloudGame, date);
  return {
    onDevice: localCatalogDates.has(date) && snapshot?.local_evidence !== 'mismatch',
    inCloud: Boolean(snapshot?.cloud_verified),
    onOtherDevice: (snapshot?.reported_on_devices.length ?? 0) > 0,
  };
}

export function isSnapshotOnDevice(
  localCatalogDates: Set<string>,
  cloudGame: CloudArchiveGameView | null,
  date: string
) {
  return snapshotAvailability(localCatalogDates, cloudGame, date).onDevice;
}

export function isSnapshotInCloud(
  localCatalogDates: Set<string>,
  cloudGame: CloudArchiveGameView | null,
  date: string
) {
  return snapshotAvailability(localCatalogDates, cloudGame, date).inCloud;
}

export function canUploadSnapshot(
  localCatalogDates: Set<string>,
  cloudGame: CloudArchiveGameView | null,
  date: string
) {
  if (!cloudGame) return false;
  const availability = snapshotAvailability(localCatalogDates, cloudGame, date);
  return availability.onDevice && !availability.inCloud;
}

export function canDownloadSnapshot(
  localCatalogDates: Set<string>,
  cloudGame: CloudArchiveGameView | null,
  date: string
) {
  const availability = snapshotAvailability(localCatalogDates, cloudGame, date);
  return availability.inCloud && !availability.onDevice;
}

export function canEvictSnapshot(
  localCatalogDates: Set<string>,
  cloudGame: CloudArchiveGameView | null,
  date: string
) {
  if (!cloudGame) return false;
  return snapshotAvailability(localCatalogDates, cloudGame, date).onDevice;
}

export function canEvictCloudSnapshot(
  localCatalogDates: Set<string>,
  cloudGame: CloudArchiveGameView | null,
  date: string
) {
  return snapshotAvailability(localCatalogDates, cloudGame, date).inCloud;
}

export function canApplySnapshot(
  localCatalogDates: Set<string>,
  cloudGame: CloudArchiveGameView | null,
  date: string
) {
  return snapshotAvailability(localCatalogDates, cloudGame, date).onDevice;
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

export function snapshotLocationKey(
  localCatalogDates: Set<string>,
  cloudGame: CloudArchiveGameView | null,
  date: string
) {
  const availability = snapshotAvailability(localCatalogDates, cloudGame, date);
  if (availability.onDevice && availability.inCloud) return 'manage.location_both';
  if (availability.onDevice) return 'manage.location_local';
  if (availability.inCloud) return 'manage.location_cloud';
  if (availability.onOtherDevice) return 'sync_settings.archives.available_other_device';
  return 'manage.location_missing';
}
