import { createApp } from "vue";
import App from "./App.vue";
import router from "./router";
import { store, key } from "./store";
import ElementPlus from "element-plus";
import "element-plus/dist/index.css";

declare global {
  interface Window {
    require: any;
  }
}
createApp(App).use(store, key).use(router).use(ElementPlus).mount("#app");
