import { error } from '@tauri-apps/plugin-log';
import {
    commands,
    DEFAULT_CONFIG,
    events,
    type Config,
} from '../bindings'
import { $t } from '../i18n'

// 定义默认配置
const defaultConfig: Config = DEFAULT_CONFIG as unknown as Config;
const { showError } = useNotification()
const config = ref(defaultConfig)
const isLoading = ref(false)

async function refreshConfig() {
    isLoading.value = true
    try {
        const result = await commands.getLocalConfig()
        if (result.status === 'error') {
            throw new Error(result.error)
        }
        config.value = result.data
    } catch (e) {
        error(`Failed to load config: ${e}`)
        showError({
            message: $t('error.config_load_failed')
        })
        // 加载失败时使用默认配置
        config.value = defaultConfig
    } finally {
        isLoading.value = false
    }
}

async function saveConfig() {
    try {
        const result = await commands.setConfig(config.value)
        if (result.status === 'error') {
            throw new Error(result.error)
        }
    } catch (e) {
        error(`Failed to set config: ${e}`)
        showError({
            message: $t('error.set_config_failed')
        })
    }
}

if (import.meta.client) {
    events.quickActionCompleted
        .listen((event) => {
            const payload = event.payload
            if (
                payload.status === 'Success' &&
                payload.operation === 'Backup'
            ) {
                void refreshConfig()
            }
        })
        .catch((err) => {
            error(`Failed to listen quick action events: ${err}`)
        })
}
// 初始加载
refreshConfig()

export function useConfig() {
    return {
        config,
        isLoading,
        refreshConfig,
        saveConfig
    }
}
