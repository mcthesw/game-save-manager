import { createI18n } from "vue-i18n";
import { invoke } from '@tauri-apps/api/tauri'
import { Config } from "./schemas/saveTypes";

const messages = await invoke("get_locale_message") as any
const config:Config = await invoke("get_local_config") 

// 如果改变了locales文件夹，开发时必须重启项目才能生效
// If the "locales" folder is changed, the project must be restarted for the changes to take effect.

export let i18n = createI18n({
    messages: messages,
    locale: config.settings.locale,
    fallbackLocale: 'zh_SIMPLIFIED',
    legacy: false,
})

export function $t(key: string) {
    // 提示类型实例化过深，但是类型我们是已知的，所以可以忽略
    // @ts-ignore
    return i18n.global.t(key)
}