import { createRouter, createWebHashHistory, RouteRecordRaw } from 'vue-router'
import Home from '../views/Home.vue'
import AddGame from '../views/AddGame.vue'
import About from '../views/About.vue'
import InOut from '../views/InOut.vue'

const routes: Array<RouteRecordRaw> = [
  {
    path: '/management',
    component: Home
  },
  {
    path: '/about',
    component: About
  }, {
    path: '/',
    redirect: '/management'
  }, {
    path: '/add-game',
    component: AddGame
  },{
    path: '/in-out',
    component: InOut
  }
]

const router = createRouter({
  history: createWebHashHistory(),
  routes
})

export default router
