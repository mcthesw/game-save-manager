import { createRouter, createWebHashHistory, RouteRecordRaw } from "vue-router";
import Home from "../views/Home.vue";
import AddGame from "../views/AddGame.vue";
import About from "../views/About.vue";
import GameManage from "../views/GameManage.vue"
import Settings from "../views/Settings.vue"

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
    path: "/home",
    component: Home,
  },
  {
    path: "/management/:name",
    component:GameManage,
  },{
    path: "/setting",
    component:Settings,
  },
];

const router = createRouter({
  history: createWebHashHistory(),
  routes,
});

export default router;
