import { error } from '@tauri-apps/plugin-log';
import { commands, DEFAULT_CONFIG, events, type Config } from '../bindings';
import { $t } from '../i18n';

const defaultConfig: Config = DEFAULT_CONFIG as unknown as Config;
const { showError } = useNotification();
const config = ref(defaultConfig);
const isLoading = ref(false);

async function refreshConfig(): Promise<boolean> {
  isLoading.value = true;
  try {
    const result = await commands.getLocalConfig();
    if (result.status === 'error') {
      throw new Error(result.error);
    }
    config.value = result.data;
    return true;
  } catch (e) {
    error(`Failed to load config: ${e}`);
    showError({
      message: $t('error.config_load_failed'),
    });
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
    showError({
      message: $t('error.set_config_failed'),
    });
    return false;
  }
}

if (import.meta.client) {
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
  return {
    config,
    isLoading,
    refreshConfig,
    saveConfig,
  };
}
