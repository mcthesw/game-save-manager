import { InjectionKey } from "vue";
import { createStore, Store } from "vuex";
import { Config, Game, Saves, Save, Games } from "../background/saveTypes";
import {default_config} from "../background/config"

// 为 store state 声明类型
export interface State {
  config: Config;
}

// 定义 injection key
export const key: InjectionKey<Store<State>> = Symbol();

export const store = createStore<State>({
  state: {
    config:default_config
  },
  mutations: {
    get_config(state, config:Config) {
      console.log("已经装载配置文件");
      state.config = config;
    },
  },
  modules: {},
});
