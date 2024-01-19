import { createApp } from "vue";
import { createI18n } from 'vue-i18n'
import App from "./App.vue";
import router from "./router";
import ElementPlus from "element-plus";
import 'element-plus/theme-chalk/dark/css-vars.css'
import "element-plus/dist/index.css";
import { createPinia } from 'pinia'
import { i18n } from "./i18n";

createApp(App).use(router).use(createPinia()).use(ElementPlus).use(i18n).mount("#app");
