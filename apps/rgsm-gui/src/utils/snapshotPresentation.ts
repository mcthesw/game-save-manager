import dayjs from 'dayjs';
import customParseFormat from 'dayjs/plugin/customParseFormat.js';

dayjs.extend(customParseFormat);

type SnapshotTime = { date: string; created_at?: number | null };

function creationTime(snapshot: SnapshotTime) {
  const time =
    snapshot.created_at == null
      ? dayjs(snapshot.date, 'YYYY-MM-DD_HH-mm-ss', true)
      : dayjs(snapshot.created_at);
  return time.isValid() ? time : null;
}

/** Chronological presentation only; cloud progress is determined by ancestry. */
export function compareSnapshotTime(left: SnapshotTime, right: SnapshotTime): number {
  const a = creationTime(left)?.valueOf() ?? -Infinity;
  const b = creationTime(right)?.valueOf() ?? -Infinity;
  return a === b ? 0 : a < b ? -1 : 1;
}

export function formatSnapshotTime(
  snapshot: SnapshotTime,
  format = 'YYYY-MM-DD HH:mm:ss'
): string | null {
  return creationTime(snapshot)?.format(format) ?? null;
}

export function snapshotDeviceName(
  deviceId: string | null | undefined,
  devices: Record<string, { name: string }> | undefined
): string | null {
  if (!deviceId) return null;
  return (
    (devices && Object.hasOwn(devices, deviceId) ? devices[deviceId]?.name.trim() : '') ||
    (deviceId.length > 8 ? `${deviceId.slice(0, 8)}…` : deviceId)
  );
}

export function findSnapshotByInput<T extends SnapshotTime & { describe: string }>(
  snapshots: T[],
  rawInput: string
): T | null {
  const input = rawInput.trim();
  if (!input) return null;
  const exact = snapshots.find((snapshot) => snapshot.date === input);
  if (exact) return exact;
  if (/^[1-9]\d*$/.test(input)) {
    return [...snapshots].slice(-10).reverse()[Number(input) - 1] ?? null;
  }
  const matches = snapshots.filter(
    (snapshot) =>
      snapshot.date.startsWith(input) ||
      formatSnapshotTime(snapshot)?.startsWith(input) ||
      snapshot.describe.toLowerCase().includes(input.toLowerCase())
  );
  return matches.length === 1 ? (matches[0] ?? null) : null;
}
