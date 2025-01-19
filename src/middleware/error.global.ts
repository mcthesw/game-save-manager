import { warn } from "@tauri-apps/plugin-log"

const { config } = useConfig()
const pages = ["/", "/About", "/AddGame", "/Settings", "/SyncSettings"]

export default defineNuxtRouteMiddleware((to, from) => {
    if (to.fullPath.startsWith("/Management") && !config.value.games.find((x) => x.name == to.params.name)) {
        warn(`Game ${to.params.name} not found`)
        return navigateTo(to)
    }
    if (!to.fullPath.startsWith("/Management") && pages.find((x) => x == to.fullPath) === undefined) {
        warn(`Page ${to.fullPath} not found`)
        return navigateTo("/")
    }
})