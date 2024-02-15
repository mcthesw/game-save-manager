import { createRouter, createWebHashHistory, RouteRecordRaw } from "vue-router";
import Home from "../views/Home.vue";
import AddGame from "../views/AddGame.vue";
import About from "../views/About.vue";
import GameManage from "../views/GameManage.vue";
import Settings from "../views/Settings.vue";
import SyncSettings from "../views/SyncSettings.vue";
import { useConfig } from "../stores/ConfigFile";

const routes: Array<RouteRecordRaw> = [
    {
        path: "/about",
        component: About,
    },
    {
        path: "/",
        redirect: "/home",
    },
    {
        path: "/add-game",
        component: AddGame,
    },
    {
        path: "/edit-game/:name",
        component: AddGame,
        name: "edit-game"
    },
    {
        path: "/home",
        component: Home,
    },
    {
        path: "/management/:name",
        beforeEnter: (to, from) => {
            const store = useConfig();
            // 避免访问到不存在的游戏
            const name = to.params.name
            if (!name || !store.games.find((x) => x.name == name)) {
                return "/home"
            }
        },
        component: GameManage,
    },
    {
        path: "/sync-settings",
        component: SyncSettings
    }
    ,
    {
        path: "/settings",
        component: Settings,
    },
];

const router = createRouter({
    history: createWebHashHistory(),
    routes,
});

export default router;
