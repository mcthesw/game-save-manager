import { createI18n } from "vue-i18n";

// Read locales from locales/*.json
import * as cn_messages from './locales/zh_SIMPLIFIED.json';

export let i18n = createI18n({
    messages: {
        cn: cn_messages,
    },
    locale: 'cn',
    legacy: false,
})

export function $t(key: string) {
    return i18n.global.t(key)
}