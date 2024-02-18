import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/tauri'
import { Config, default_config } from '../schemas/saveTypes'
import { show_error } from '../utils/notifications';
import { $t } from '../i18n';

export const useConfig = defineStore('config', {
    state: () => (default_config),
    actions: {
        refresh() {
            invoke("get_local_config").then((c) => {
                console.log("Get local config:", c);
                this.$state = c as Config
            }).catch((e) => { 
                show_error($t('error.config_load_failed'))
                console.log(e) 
            })
        }
    }
});