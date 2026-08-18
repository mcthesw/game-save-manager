export const VITE_ORIGIN = 'http://localhost:5173';
export const VITE_PORT = 5173;

export const GAME_NAME = 'Echo Keep';
export const STORAGE_KEY = 'Echo Keep';
export const SAVE_FILE_NAME = 'progress.txt';

export const DEVICE_A_ID = 'e2e-device-a';
export const DEVICE_B_ID = 'e2e-device-b';
export const DEVICE_A_NAME = 'E2E Device A';
export const DEVICE_B_NAME = 'E2E Device B';

export const PARENT_SNAPSHOT_ID = '2026-01-01_12-00-00';
export const CHILD_SNAPSHOT_ID = '2026-01-02_12-00-00';
export const PARENT_ARCHIVE_HASH = '66fd5ad032b19c61';
export const CHILD_ARCHIVE_HASH = 'b20b92239c7fb335';
export const PARENT_ARCHIVE_SIZE = 143;
export const CHILD_ARCHIVE_SIZE = 150;
export const PARENT_SAVE_BYTES = 'parent-save-v1\n';
export const CHILD_SAVE_BYTES = 'child-save-v2-forward\n';

export const V2_ACTIVE_ERROR =
  'Legacy cloud synchronization is unavailable after V2 Cloud Library activation';

export function encodeDeviceId(deviceId: string): string {
  return Buffer.from(deviceId, 'utf8').toString('hex');
}

export function deviceProfileFileName(deviceId: string): string {
  return `${encodeDeviceId(deviceId)}.json`;
}
