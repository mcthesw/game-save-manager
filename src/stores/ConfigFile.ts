import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/tauri'
import { Config, default_config } from '../schemas/saveTypes'

export const useConfig = defineStore('config', {
    state: () => (default_config),
    actions: {
        refresh() {
            invoke("get_local_config").then((c) => {
                console.log("Get local config:", c);
                this.$state = c as Config
            })
        }
    }
});