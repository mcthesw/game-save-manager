import type { CloudArchiveGameView, CloudArchiveSnapshotView } from '../bindings';
import { $t } from '../i18n';

export function formatCloudArchiveBytes(bytes: number | null | undefined) {
  if (!bytes) return $t('sync_settings.archives.size_unknown');
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** index).toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
}

export function cloudArchiveTransferKey(gameId: string, snapshotId: string) {
  return `${gameId}\0${snapshotId}`;
}

export function cloudArchiveCatchUpPreview(game: CloudArchiveGameView | null) {
  const snapshots =
    game?.snapshots.filter((snapshot) => snapshot.cloud_verified && !snapshot.local_verified) ?? [];
  return {
    count: snapshots.length,
    size: snapshots.reduce((total, snapshot) => total + (snapshot.size ?? 0), 0),
  };
}

export function cloudArchiveAvailabilityLabel(snapshot: CloudArchiveSnapshotView) {
  if (snapshot.local_verified && snapshot.cloud_verified) {
    return $t('sync_settings.archives.available_both');
  }
  if (snapshot.local_verified) return $t('sync_settings.archives.available_local');
  if (snapshot.cloud_verified) return $t('sync_settings.archives.available_cloud');
  if (snapshot.reported_on_devices.length > 0) {
    return $t('sync_settings.archives.available_other_device');
  }
  return $t('sync_settings.archives.unavailable');
}

export function cloudArchiveAvailabilityType(snapshot: CloudArchiveSnapshotView) {
  if (snapshot.local_verified && snapshot.cloud_verified) return 'success';
  if (snapshot.local_verified || snapshot.cloud_verified) return 'primary';
  if (snapshot.reported_on_devices.length > 0) return 'warning';
  return 'info';
}

export function canProtectCloudArchiveSnapshot(snapshot: CloudArchiveSnapshotView) {
  return (
    snapshot.created_by === 'Timer' ||
    snapshot.created_by === 'ProcessStart' ||
    snapshot.created_by === 'ProcessExit' ||
    snapshot.created_by === 'ProcessInterval'
  );
}
