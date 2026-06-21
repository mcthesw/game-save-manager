import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import VueRouter from 'vue-router/vite';
import vue from '@vitejs/plugin-vue';
import AutoImport from 'unplugin-auto-import/vite';
import Components from 'unplugin-vue-components/vite';
import { defineConfig } from 'vite';

const appRoot = dirname(fileURLToPath(import.meta.url));
const sourcePath = (path: string) => resolve(appRoot, path).replace(/\\/g, '/');
const kebabCase = (name: string) => name.replace(/([a-z0-9])([A-Z])/g, '$1-$2').toLowerCase();
const elementPlusModuleByComponent: Record<string, string> = {
  ElAside: 'container',
  ElAutoResizer: 'table-v2',
  ElAvatarGroup: 'avatar',
  ElBreadcrumbItem: 'breadcrumb',
  ElButtonGroup: 'button',
  ElCarouselItem: 'carousel',
  ElCheckboxButton: 'checkbox',
  ElCheckboxGroup: 'checkbox',
  ElCollapseItem: 'collapse',
  ElDescriptionsItem: 'descriptions',
  ElDropdownItem: 'dropdown',
  ElDropdownMenu: 'dropdown',
  ElFooter: 'container',
  ElHeader: 'container',
  ElMain: 'container',
  ElFormItem: 'form',
  ElMenuItem: 'menu',
  ElMenuItemGroup: 'menu',
  ElOption: 'select',
  ElOptionGroup: 'select',
  ElRadioButton: 'radio',
  ElRadioGroup: 'radio',
  ElSkeletonItem: 'skeleton',
  ElStep: 'steps',
  ElSubMenu: 'menu',
  ElTableColumn: 'table',
  ElTabPane: 'tabs',
  ElTimelineItem: 'timeline',
  ElTourStep: 'tour',
};

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
  './src/bindings.ts',
].map(sourcePath);

const elementPlusOptimizationEntries = [
  'element-plus/es/components/alert/index.mjs',
  'element-plus/es/components/alert/style/css',
  'element-plus/es/components/aside/style/css',
  'element-plus/es/components/button/index.mjs',
  'element-plus/es/components/button/style/css',
  'element-plus/es/components/card/index.mjs',
  'element-plus/es/components/card/style/css',
  'element-plus/es/components/checkbox/index.mjs',
  'element-plus/es/components/checkbox/style/css',
  'element-plus/es/components/collapse/index.mjs',
  'element-plus/es/components/collapse/style/css',
  'element-plus/es/components/collapse-item/style/css',
  'element-plus/es/components/collapse-transition/index.mjs',
  'element-plus/es/components/collapse-transition/style/css',
  'element-plus/es/components/container/index.mjs',
  'element-plus/es/components/container/style/css',
  'element-plus/es/components/descriptions/index.mjs',
  'element-plus/es/components/descriptions/style/css',
  'element-plus/es/components/descriptions-item/style/css',
  'element-plus/es/components/dialog/index.mjs',
  'element-plus/es/components/dialog/style/css',
  'element-plus/es/components/divider/index.mjs',
  'element-plus/es/components/divider/style/css',
  'element-plus/es/components/drawer/index.mjs',
  'element-plus/es/components/drawer/style/css',
  'element-plus/es/components/empty/index.mjs',
  'element-plus/es/components/empty/style/css',
  'element-plus/es/components/form/index.mjs',
  'element-plus/es/components/form/style/css',
  'element-plus/es/components/form-item/style/css',
  'element-plus/es/components/icon/index.mjs',
  'element-plus/es/components/icon/style/css',
  'element-plus/es/components/input/index.mjs',
  'element-plus/es/components/input/style/css',
  'element-plus/es/components/input-number/index.mjs',
  'element-plus/es/components/input-number/style/css',
  'element-plus/es/components/link/index.mjs',
  'element-plus/es/components/link/style/css',
  'element-plus/es/components/loading/index.mjs',
  'element-plus/es/components/loading/style/css',
  'element-plus/es/components/main/style/css',
  'element-plus/es/components/menu/index.mjs',
  'element-plus/es/components/menu/style/css',
  'element-plus/es/components/menu-item/style/css',
  'element-plus/es/components/message-box/index.mjs',
  'element-plus/es/components/message-box/style/css',
  'element-plus/es/components/option/style/css',
  'element-plus/es/components/pagination/index.mjs',
  'element-plus/es/components/pagination/style/css',
  'element-plus/es/components/popconfirm/index.mjs',
  'element-plus/es/components/popconfirm/style/css',
  'element-plus/es/components/popover/index.mjs',
  'element-plus/es/components/popover/style/css',
  'element-plus/es/components/radio/index.mjs',
  'element-plus/es/components/radio-button/style/css',
  'element-plus/es/components/radio-group/style/css',
  'element-plus/es/components/row/index.mjs',
  'element-plus/es/components/row/style/css',
  'element-plus/es/components/scrollbar/index.mjs',
  'element-plus/es/components/scrollbar/style/css',
  'element-plus/es/components/select/index.mjs',
  'element-plus/es/components/select/style/css',
  'element-plus/es/components/sub-menu/style/css',
  'element-plus/es/components/switch/index.mjs',
  'element-plus/es/components/switch/style/css',
  'element-plus/es/components/tab-pane/style/css',
  'element-plus/es/components/table/index.mjs',
  'element-plus/es/components/table/style/css',
  'element-plus/es/components/table-column/style/css',
  'element-plus/es/components/table-v2/index.mjs',
  'element-plus/es/components/table-v2/style/css',
  'element-plus/es/components/tabs/index.mjs',
  'element-plus/es/components/tabs/style/css',
  'element-plus/es/components/tag/index.mjs',
  'element-plus/es/components/tag/style/css',
  'element-plus/es/components/tooltip/index.mjs',
  'element-plus/es/components/tooltip/style/css',
  'element-plus/es/components/tree/index.mjs',
  'element-plus/es/components/tree/style/css',
];

function resolveElementPlusComponent(name: string) {
  if (!name.startsWith('El') || /^ElIcon[A-Z]/.test(name)) {
    return;
  }

  const componentName = kebabCase(name.slice(2));
  const moduleName = elementPlusModuleByComponent[name] ?? componentName;
  return {
    name,
    from: `element-plus/es/components/${moduleName}/index.mjs`,
    sideEffects:
      name === 'ElAutoResizer'
        ? undefined
        : `element-plus/es/components/${componentName}/style/css`,
  };
}

function resolveElementPlusDirective(name: string) {
  const directives: Record<string, { importName: string; styleName: string }> = {
    Loading: { importName: 'ElLoadingDirective', styleName: 'loading' },
    Popover: { importName: 'ElPopoverDirective', styleName: 'popover' },
    InfiniteScroll: { importName: 'ElInfiniteScroll', styleName: 'infinite-scroll' },
  };
  const directive = directives[name];
  if (!directive) {
    return;
  }

  return {
    name: directive.importName,
    from: `element-plus/es/components/${directive.styleName}/index.mjs`,
    sideEffects: `element-plus/es/components/${directive.styleName}/style/css`,
  };
}

export default defineConfig({
  clearScreen: false,
  publicDir: 'src/public',
  plugins: [
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
      resolvers: [
        { type: 'component', resolve: resolveElementPlusComponent },
        { type: 'directive', resolve: resolveElementPlusDirective },
      ],
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
      '@tauri-apps/plugin-log',
      '@tauri-apps/api/event',
      '@tauri-apps/api/core',
      '@tauri-apps/api/webviewWindow',
      '@element-plus/icons-vue',
      'dayjs/plugin/*.js',
      'vuedraggable',
      '@vue-flow/core',
      '@vue-flow/background',
      ...elementPlusOptimizationEntries,
    ],
    exclude: ['vue-i18n'],
  },
});
