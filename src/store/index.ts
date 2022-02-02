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
    version: "0.1.0",
    save_file: {
      save_version: "V0",
      games: {
        default: [
          { name: "黑魂3", save_path: "鬼", icon: "怕" },
          { name: "星露谷物语", save_path: "鬼", icon: "怕" },
          { name: "传送门2", save_path: "鬼", icon: "怕" },
          { name: "黑魂3", save_path: "鬼", icon: "怕" },
          { name: "星露谷物语", save_path: "鬼", icon: "怕" },
          { name: "传送门2", save_path: "鬼", icon: "怕" },
          { name: "黑魂3", save_path: "鬼", icon: "怕" },
          { name: "星露谷物语", save_path: "鬼", icon: "怕" },
          { name: "传送门2", save_path: "鬼", icon: "怕" },
          { name: "黑魂3", save_path: "鬼", icon: "怕" },
          { name: "星露谷物语", save_path: "鬼", icon: "怕" },
          { name: "传送门2", save_path: "鬼", icon: "怕" },
          { name: "黑魂3", save_path: "鬼", icon: "怕" },
          { name: "星露谷物语", save_path: "鬼", icon: "怕" },
          { name: "传送门2", save_path: "鬼", icon: "怕" },
          { name: "黑魂3", save_path: "鬼", icon: "怕" },
          { name: "星露谷物语", save_path: "鬼", icon: "怕" },
          { name: "传送门2", save_path: "鬼", icon: "怕" },
        ],
      },
    },
  },
  mutations: {
    get_saved_games(state, saves_from_file: GamesInfo) {},
  },
  actions: {
    get_saved_games(context) {
      // TODO:读取json文件，通过commit放入store
    },
  },
  modules: {},
});
