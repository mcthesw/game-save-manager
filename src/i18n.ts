import { createI18n } from "vue-i18n";
import { invoke } from '@tauri-apps/api/tauri'
import { Config } from "./schemas/saveTypes";

const messages = await invoke("get_locale_message") as any
const config:Config = await invoke("get_local_config") 

export let i18n = createI18n({
    messages: messages,
    locale: config.settings.locale,
    fallbackLocale: 'zh_SIMPLIFIED',
    legacy: false,
})

export function $t(key: string) {
    return i18n.global.t(key)
}