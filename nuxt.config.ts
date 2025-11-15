// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  srcDir: 'src',
  compatibilityDate: '2024-11-01',
  devtools: { enabled: process.env.NUXT_DEVTOOLS === 'true' },
  ssr: false,
  devServer: { host: process.env.TAURI_DEV_HOST || 'localhost' },
  modules: ['@vueuse/nuxt', '@element-plus/nuxt', '@nuxt/eslint'],
  imports: {
    dirs: ['src/composables'],
  },
  vite: {
    // Better support for Tauri CLI output
    clearScreen: false,
    // Enable environment variables
    // Additional environment variables can be found at
    // https://v2.tauri.app/reference/environment-variables/
    envPrefix: ['VITE_', 'TAURI_'],
    server: {
      // Tauri requires a consistent port
      strictPort: true,
    },
  },
  app: {
    pageTransition: { name: 'page', mode: 'out-in' },
  },
  dir: {
    public: 'src/public',
    modules: 'src/modules',
    shared: 'src/shared',
  },
});
