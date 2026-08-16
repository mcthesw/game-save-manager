import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import VueRouter from 'vue-router/vite';
import tailwindcss from '@tailwindcss/vite';
import vue from '@vitejs/plugin-vue';
import AutoImport from 'unplugin-auto-import/vite';
import Components from 'unplugin-vue-components/vite';
import { defineConfig } from 'vite';

const appRoot = dirname(fileURLToPath(import.meta.url));
const sourcePath = (path: string) => resolve(appRoot, path).replace(/\\/g, '/');
type HostConfig = { port: number; api_token: string };
let hostConfig: HostConfig | undefined;
try {
  hostConfig = JSON.parse(
    readFileSync(resolve(appRoot, '../../.rgsm-dev/app-data/GameSaveManager.host.json'), 'utf8')
  ) as HostConfig;
} catch {
  // Tauri starts its embedded Host after Vite; the WebView receives a direct runtime URL.
}

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
  './src/main.ts',
  './src/App.vue',
  './src/pages/**/*.vue',
  './src/components/**/*.vue',
  './src/composables/**/*.ts',
  './src/i18n.ts',
  './src/api/generated/sdk.gen.ts',
].map(sourcePath);

export default defineConfig({
  clearScreen: false,
  publicDir: 'src/public',
  plugins: [
    tailwindcss(),
    VueRouter({
      routesFolder: 'src/pages',
      dts: 'typed-router.d.ts',
    }),
    vue(),
    AutoImport({
      imports: [
        'vue',
        'vue-router',
        '@vueuse/core',
        {
          '@/i18n': ['$t'],
          '@/router': ['navigateTo'],
        },
      ],
      dirs: ['src/composables'],
      dts: false,
    }),
    Components({
      dirs: ['src/components'],
      dts: 'components.d.ts',
    }),
  ],
  resolve: {
    alias: {
      '@': sourcePath('./src'),
      '~': sourcePath('./src'),
    },
  },
  envPrefix: ['VITE_', 'TAURI_'],
  cacheDir: '../../node_modules/.vite/rgsm-gui',
  server: {
    port: 5173,
    strictPort: true,
    host: process.env.TAURI_DEV_HOST || 'localhost',
    watch: {
      ignored: generatedFilePatterns,
    },
    proxy: hostConfig
      ? {
          '/api/v1': {
            target: `http://127.0.0.1:${hostConfig.port}`,
            changeOrigin: true,
            configure(proxy) {
              proxy.on('proxyReq', (request) => {
                if (!request.hasHeader('Authorization')) {
                  request.setHeader('Authorization', `Bearer ${hostConfig.api_token}`);
                }
              });
            },
          },
        }
      : undefined,
  },
  build: {
    outDir: 'dist',
    target: 'baseline-widely-available',
  },
  optimizeDeps: {
    entries: viteOptimizationEntries,
    include: [
      'dayjs',
      'uuid',
      'dayjs/plugin/*.js',
      'vuedraggable',
      '@vue-flow/core',
      '@vue-flow/background',
    ],
    exclude: ['vue-i18n'],
  },
});
