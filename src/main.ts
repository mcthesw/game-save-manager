import { createApp } from "vue";
import App from "./App.vue";
import router from "./router";
import ElementPlus from "element-plus";
import 'element-plus/theme-chalk/dark/css-vars.css'
import "element-plus/dist/index.css";
import { createPinia } from 'pinia'

createApp(App).use(router).use(createPinia()).use(ElementPlus).mount("#app");
