import { createRouter, createWebHashHistory, RouteRecordRaw } from "vue-router";
import Home from "../views/Home.vue";
import AddGame from "../views/AddGame.vue";
import About from "../views/About.vue";
import GameManage from "../views/GameManage.vue"

const routes: Array<RouteRecordRaw> = [
  {
    path: "/about",
    component: About,
  },
  {
    path: "/",
    redirect: "/management",
  },
  {
    path: "/add-game",
    component: AddGame,
  },
  {
    path: "/home",
    component: Home,
  },
  {
    path: "/management/:name",
    component:GameManage,
  },
];

const router = createRouter({
  history: createWebHashHistory(),
  routes,
});

export default router;
