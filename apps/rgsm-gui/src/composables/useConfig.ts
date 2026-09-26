import { error } from '../utils/logger';
import { isEqual } from 'lodash-unified';
import { shareUnchangedItems } from '../utils/stableCollections';
import { commands, DEFAULT_CONFIG, type Config, type DeviceGameStatus } from '../api/commands';
import { $t } from '../i18n';

const config = ref<Config>(structuredClone(DEFAULT_CONFIG));
const deviceGameStatuses = ref<DeviceGameStatus[]>([]);
const isLoading = ref(false);
let latestRead: Promise<boolean> | null = null;
let readSequence = 0;

async function whenConfigReady(): Promise<boolean> {
  while (true) {
    const pending = latestRead ?? refreshConfig();
    const loaded = await pending;
    if (pending === latestRead) return loaded;
  }
}
async function readConfig(libraryOnly: boolean, isCurrent = () => true): Promise<boolean> {
  if (!isCurrent()) return false;
  const sequence = ++readSequence;
  const externalIsCurrent = isCurrent;
  isCurrent = () => sequence === readSequence && externalIsCurrent();
  isLoading.value = true;
  try {
    const result = await commands.getLocalConfig();
    if (result.status === 'error') {
      throw new Error(result.error);
    }
    const statuses = await commands.getCurrentDeviceGameStatuses();
    if (statuses.status === 'error') throw new Error(statuses.error);
    if (!isCurrent()) return false;
    if (libraryOnly) {
      // Cloud metadata refresh must not replace local settings being edited.
      config.value.games = shareUnchangedItems(
        config.value.games,
        result.data.games,
        (game) => game.storage_key || game.name
      );
      if (!isEqual(config.value.devices, result.data.devices)) {
        config.value.devices = result.data.devices;
      }
    } else {
      config.value = result.data;
    }
    if (!isEqual(deviceGameStatuses.value, statuses.data)) {
      deviceGameStatuses.value = statuses.data;
    }
    return true;
  } catch (e) {
    if (!isCurrent()) return false;
    error(`Failed to load config: ${e}`);
    notifyError($t('error.config_load_failed'));
    return false;
  } finally {
    if (sequence === readSequence) isLoading.value = false;
  }
}

function refreshConfig(): Promise<boolean> {
  latestRead = readConfig(false);
  return latestRead;
}

function refreshLibraryConfig(isCurrent: () => boolean): Promise<boolean> {
  latestRead = readConfig(true, isCurrent);
  return latestRead;
}

async function saveConfig(): Promise<boolean> {
  try {
    const result = await commands.setConfig(config.value);
    if (result.status === 'error') {
      throw new Error(result.error);
    }
    return true;
  } catch (e) {
    error(`Failed to set config: ${e}`);
    notifyError($t('error.set_config_failed'));
    return false;
  }
}

void refreshConfig();

export function useConfig() {
  const isGameVisible = (gameId: string | undefined, fallbackName?: string) => {
    const identity = gameId || fallbackName;
    return deviceGameStatuses.value.find((status) => status.game_id === identity)?.visible ?? true;
  };
  return {
    config,
    deviceGameStatuses,
    isGameVisible,
    isLoading,
    refreshConfig,
    refreshLibraryConfig,
    saveConfig,
    whenConfigReady,
  };
}
