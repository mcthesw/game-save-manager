/*
 * @Author: PlanC
 * @Date: 2022-09-28 12:13:50
 * @LastEditTime: 2022-09-28 12:49:47
 * @FilePath: \game-save-manager\src\store\index.ts
 */

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
  getters: {
	get_config: state => state.config,
  },
  mutations: {
    get_config(state, config:Config) {
      console.log("已经装载配置文件");
      state.config = config;
    },
  },
  modules: {},
});
