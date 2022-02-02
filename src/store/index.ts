import { InjectionKey } from "vue";
import { createStore, Store } from "vuex";

// 为 store state 声明类型
interface GameInfo {
  name: string;
  save_path: string;
  game_path?: string;
  icon: string;
}
interface GamesInfo {
  save_version: string;
  games: {
    default: [GameInfo];
    custom?: [GameInfo];
  };
}
export interface State {
  version: string;
  save_file: GamesInfo | {};
}

// 定义 injection key
export const key: InjectionKey<Store<State>> = Symbol();

export const store = createStore<State>({
  state: {
    version: "0.1.0",
    save_file: {},
  },
  mutations: {
    get_saved_games(state: State, saves_from_file: GamesInfo) {},
  },
  actions: {
    get_saved_games(context) {
      // TODO:读取json文件，通过commit放入store
    },
  },
  modules: {},
});
