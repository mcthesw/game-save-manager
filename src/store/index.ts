import { createStore } from "vuex";

export default createStore({
  state: {
    version: "0.1.0",
    save_file: {},
  },
  mutations: {
    get_saved_games(state, saves_from_file) {},
  },
  actions: {
    get_saved_games(context) {
      // TODO:读取json文件，通过commit放入store
    },
  },
  modules: {},
});
