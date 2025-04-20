import { createI18n } from 'vue-i18n'

import en_US from '../locales/en_US.json'
import fr from '../locales/fr.json'
import ko from '../locales/ko.json'
import nb_NO from '../locales/nb_NO.json'
import ta from '../locales/ta.json'
import uk from '../locales/uk.json'
import zh_SIMPLIFIED from '../locales/zh_SIMPLIFIED.json'

// 导出 i18n 实例
export const i18n = createI18n({
    messages: {
        'en_US': en_US,
        'fr': fr,
        'ko': ko,
        // 'nb_NO': nb_NO,
        'ta': ta,
        'uk': uk,
        'zh_SIMPLIFIED': zh_SIMPLIFIED,
    },
    locale: 'zh_SIMPLIFIED', // 默认语言
    fallbackLocale: 'en_US', // 备用语言改为英语
    legacy: false,
})

// 导出简单的翻译函数
export function $t(key: string) {
    return i18n.global.t(key)
}

// 导出所有支持的语言及其本地化名称
export function getSupportedLanguages() {
    const messages = i18n.global.messages.value as Record<string, any>;
    const locales = Object.keys(messages);
    
    return locales.map(locale => {
        return {
            code: locale,
            name: messages[locale]?.settings?.locale_name || locale
        };
    });
}
