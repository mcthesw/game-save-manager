import { InjectionKey } from "vue";
import { createStore, Store } from "vuex";

// 为 store state 声明类型
interface OneGameInfo {
  name: string;
  save_path: string;
  game_path?: string;
  icon: string;
}
interface OneGameInfoArray {
  [index: number]: OneGameInfo;
}
interface GamesInfo {
  save_version: string;
  games: {
    default: OneGameInfoArray;
    custom?: OneGameInfoArray;
  };
}
export interface State {
  version: string;
  save_file: GamesInfo;
}

// 定义 injection key
export const key: InjectionKey<Store<State>> = Symbol();

export const store = createStore<State>({
  state: {
    version: "0.0.0",
    save_file: {
      save_version: "V0",
      games: {
        default: [
          { name: "黑魂3", save_path: "鬼", icon: "怕" },
          { name: "星露谷物语", save_path: "鬼", icon: "怕" },
          { name: "传送门2", save_path: "鬼", icon: "怕" },
          { name: "帝国时代4", save_path: "鬼", icon: "怕" },
        ],
      },
    },
  },
  mutations: {
    get_saved_games(state, saves_from_file: GamesInfo) {
      // TODO: 读取json文件，通过commit放入store
      // TODO: 流程基本是这样的，先读取本地存在的游戏存档集合，然后把其中有的游戏都加到state内
    },
    get_config(state, config) {
      console.log("获取到配置文件:", config);
      state.version = config.version;
    },
  },
  modules: {},
});
