import { createI18n } from 'vue-i18n'
import { commands } from './bindings'

// 创建 i18n 实例
const initI18n = async () => {
    try {
        const messagesResult = await commands.getLocaleMessage()
        const configResult = await commands.getLocalConfig()

        if (messagesResult.status !== 'ok' || configResult.status !== 'ok') {
            throw new Error('Failed to load i18n resources')
        }

        const messages = messagesResult.data
        const config = configResult.data

        // 解析消息
        const parsedMessages: Record<string, any> = {}
        for (const [key, value] of Object.entries(messages)) {
            parsedMessages[key] = JSON.parse(value)
        }

        return createI18n({
            messages: parsedMessages,
            locale: config.settings.locale,
            fallbackLocale: 'zh_SIMPLIFIED',
            legacy: false,
        })
    } catch (error) {
        console.error('Failed to initialize i18n:', error)
        // 返回一个基础的 i18n 实例作为后备
        return createI18n({
            messages: {},
            locale: 'zh_SIMPLIFIED',
            fallbackLocale: 'zh_SIMPLIFIED',
            legacy: false,
        })
    }
}

// 导出 i18n 实例
export const i18n = await initI18n()

// 导出简单的翻译函数
export function $t(key: string) {
    return i18n.global.t(key)
}
