import { error } from '@tauri-apps/plugin-log';
import { commands, DEFAULT_CONFIG, events, type Config, type DeviceGameStatus } from '../bindings';
import { $t } from '../i18n';

const defaultConfig: Config = DEFAULT_CONFIG as unknown as Config;
const config = ref(defaultConfig);
const deviceGameStatuses = ref<DeviceGameStatus[]>([]);
const isLoading = ref(false);

async function refreshConfig(): Promise<boolean> {
  isLoading.value = true;
  try {
    const result = await commands.getLocalConfig();
    if (result.status === 'error') {
      throw new Error(result.error);
    }
    config.value = result.data;
    const statuses = await commands.getCurrentDeviceGameStatuses();
    deviceGameStatuses.value =
      statuses.status === 'ok'
        ? statuses.data
        : config.value.games.map((game) => ({
            game_id: game.storage_key || game.name,
            managed: true,
            visible: true,
          }));
    return true;
  } catch (e) {
    error(`Failed to load config: ${e}`);
    notifyError($t('error.config_load_failed'));
    config.value = defaultConfig;
    return false;
  } finally {
    isLoading.value = false;
  }
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

if (typeof window !== 'undefined') {
  events.quickActionCompleted
    .listen((event) => {
      const payload = event.payload;
      if (payload.status === 'Success' && payload.operation === 'Backup') {
        void refreshConfig();
      }
    })
    .catch((err) => {
      error(`Failed to listen quick action events: ${err}`);
    });
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
    saveConfig,
  };
}
