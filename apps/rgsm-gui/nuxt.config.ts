// https://nuxt.com/docs/api/configuration/nuxt-config
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const appRoot = dirname(fileURLToPath(import.meta.url));
const sourcePath = (path: string) => resolve(appRoot, path).replace(/\\/g, '/');

const generatedFilePatterns = [
  '.data/**',
  '.nuxt/**',
  '.output/**',
  'dist/**',
  'node_modules/**',
  'src-tauri/gen/**',
  'src-tauri/target/**',
  '**/.data/**',
  '**/.nuxt/**',
  '**/.output/**',
  '**/dist/**',
  '**/node_modules/**',
  '**/src-tauri/gen/**',
  '**/src-tauri/target/**',
];

const viteOptimizationEntries = [
  './src/App.vue',
  './src/layouts/**/*.vue',
  './src/pages/**/*.vue',
  './src/components/**/*.vue',
  './src/composables/**/*.ts',
  './src/i18n.ts',
  './src/bindings.ts',
].map(sourcePath);

export default defineNuxtConfig({
  srcDir: 'src',
  compatibilityDate: '2024-11-01',
  devtools: { enabled: process.env.NUXT_DEVTOOLS === 'true' },
  ssr: false,
  devServer: { host: process.env.TAURI_DEV_HOST || 'localhost' },
  ignore: generatedFilePatterns,
  watchers: {
    chokidar: {
      ignored: generatedFilePatterns,
    },
  },
  experimental: {
    watcher: 'chokidar-granular',
  },
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
    cacheDir: '../../node_modules/.vite/rgsm-gui',
    server: {
      // Tauri requires a consistent port
      strictPort: true,
      watch: {
        ignored: generatedFilePatterns,
      },
    },
    optimizeDeps: {
      entries: viteOptimizationEntries,
      include: [
        'dayjs', // CJS
        'uuid',
        '@tauri-apps/plugin-log',
        '@tauri-apps/api/event',
        '@tauri-apps/api/core',
        '@tauri-apps/api/webviewWindow',
        '@element-plus/icons-vue',
        'vue-i18n',
        'dayjs/plugin/*.js',
        'vuedraggable', // CJS
        '@vue-flow/core',
        '@vue-flow/background',
      ],
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
