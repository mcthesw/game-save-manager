import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/tauri'
import { Config, default_config } from '../schemas/saveTypes'
import { show_error, show_success } from '../utils/notifications';
import { $t } from '../i18n';

export const useConfig = defineStore('config', {
    state: () => (default_config),
    actions: {
        async refresh() {
            try {
                const c = await invoke("get_local_config");
                console.log("Get local config:", c);
                this.$state = c as Config;
            } catch (e) {
                console.log(e);
                show_error($t('error.config_load_failed'));
            }
        },
        async save() {
            try {
                await invoke("set_config", { config: this.$state });
                show_success($t("settings.submit_success"));
            } catch (e) {
                console.log(e);
                show_error($t("error.set_config_failed"));
            }
        }
    }
});