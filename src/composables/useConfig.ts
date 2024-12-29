import { commands, DEFAULT_CONFIG, type Config } from '../bindings'
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
        console.error('配置加载失败:', e)
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
        console.error('配置保存失败:', e)
        showError({
            message: $t('error.set_config_failed')
        })
    }
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
