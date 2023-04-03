import { defineStore } from 'pinia'

export const useConfig = defineStore('config', {
    state: () => ({
        version:"0.4.0"
    })
});