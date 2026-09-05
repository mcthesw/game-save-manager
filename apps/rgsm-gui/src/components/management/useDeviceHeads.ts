import { computed, type Ref } from 'vue';
import { formatSnapshotTime, snapshotDeviceName } from '../../utils/snapshotPresentation';
import type {
  CloudArchiveGameView,
  Config,
  Device,
  GameSnapshots,
  Snapshot,
} from '../../api/commands';
import { devicePositions } from '../../utils/devicePositions';
import { $t } from '../../i18n';

type GameSnapshotsWithDeviceHeads = GameSnapshots & {
  device_heads?: Record<string, string | null | undefined>;
};

export interface DeviceHeadEntry {
  deviceId: string;
  deviceName: string;
  label: string;
  date: string;
  description: string;
  shortTime: string;
  fullText: string;
  isCurrentDevice: boolean;
}

export interface BranchDeviceHeadMarker {
  deviceId: string;
  date: string;
  label: string;
  isCurrentDevice: boolean;
  tooltip: string;
}

/**
 * Per-device HEAD pointers: which snapshot each device currently sits on.
 * The current device's HEAD is the amber "you are here" pin in table/branch views.
 */
export function useDeviceHeads(deps: {
  gameSnapshots: Ref<GameSnapshots | null>;
  tableData: Ref<Snapshot[]>;
  currentDevice: Ref<Device | null>;
  config: Ref<Config>;
  cloudGame: Ref<CloudArchiveGameView | null>;
}) {
  const { gameSnapshots, tableData, currentDevice, config, cloudGame } = deps;

  function resolveDeviceDisplayName(deviceId: string) {
    if (currentDevice.value?.id === deviceId && currentDevice.value.name.trim()) {
      return currentDevice.value.name;
    }

    return (
      snapshotDeviceName(deviceId, config.value.devices) ?? $t('manage.unknown_snapshot_device')
    );
  }

  const deviceHeadMap = computed<Record<string, string>>(() => {
    const snapshots = gameSnapshots.value as GameSnapshotsWithDeviceHeads | null;
    return devicePositions(
      snapshots?.device_heads,
      cloudGame.value?.device_heads,
      currentDevice.value?.id
    );
  });

  const currentHead = computed(() => {
    const deviceId = currentDevice.value?.id;
    if (!deviceId) return null;
    return deviceHeadMap.value[deviceId] ?? null;
  });

  const headEntries = computed<DeviceHeadEntry[]>(() => {
    const currentDeviceId = currentDevice.value?.id;
    return Object.entries(deviceHeadMap.value)
      .map(([deviceId, date]) => {
        const snapshot = tableData.value.find((item) => item.date === date) ?? null;
        const description = snapshot?.describe?.trim() || '';
        const shortTime =
          formatSnapshotTime(snapshot ?? { date }, 'MM/DD HH:mm') ??
          $t('manage.unknown_snapshot_time');
        const fullTime =
          formatSnapshotTime(snapshot ?? { date }) ?? $t('manage.unknown_snapshot_time');
        const isCurrentDevice = deviceId === currentDeviceId;
        const label = isCurrentDevice
          ? $t('manage.current_position')
          : resolveDeviceDisplayName(deviceId);
        const fullText = description
          ? `${label} · ${description} (${fullTime})`
          : `${label} · ${fullTime}`;

        return {
          deviceId,
          deviceName: resolveDeviceDisplayName(deviceId),
          label,
          date,
          description,
          shortTime,
          fullText,
          isCurrentDevice,
        };
      })
      .sort((left, right) => {
        if (left.isCurrentDevice !== right.isCurrentDevice) {
          return left.isCurrentDevice ? -1 : 1;
        }
        return left.deviceName.localeCompare(right.deviceName);
      });
  });

  const branchDeviceHeads = computed<BranchDeviceHeadMarker[]>(() =>
    headEntries.value.map((entry) => ({
      deviceId: entry.deviceId,
      date: entry.date,
      label: entry.isCurrentDevice ? $t('manage.head') : entry.deviceName,
      isCurrentDevice: entry.isCurrentDevice,
      tooltip: entry.fullText,
    }))
  );

  return {
    deviceHeadMap,
    currentHead,
    headEntries,
    branchDeviceHeads,
    resolveDeviceDisplayName,
  };
}
