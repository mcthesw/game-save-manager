import { createRouter, createWebHashHistory, RouteRecordRaw } from "vue-router";
import Home from "../views/Home.vue";
import AddGame from "../views/AddGame.vue";
import About from "../views/About.vue";
import GameManage from "../views/GameManage.vue";
import Settings from "../views/Settings.vue";
import ChangeGameInfo from "../views/ChangeGameInfo.vue";

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
        path: "/change-game-info",
        component: ChangeGameInfo,
    },
    {
        path: "/home",
        component: Home,
    },
    {
        path: "/management/:name",
        component: GameManage,
    },
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
