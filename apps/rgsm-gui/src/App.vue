<script setup lang="ts">
import 'element-plus/theme-chalk/dark/css-vars.css';

import { LoaderCircle } from '@lucide/vue';
import { useDark } from '@vueuse/core';
import ActivityDrawer from './components/ActivityDrawer.vue';
import DeviceSetupDialog from './components/DeviceSetupDialog.vue';
import KFeedbackHost from './ui/kit/KFeedbackHost.vue';
import KToaster from './ui/kit/KToaster.vue';
import { commands, events } from './api/commands';
import type { Device } from './api/commands';
import {
  notifyInfo,
  notifyWarning,
  notifyError,
  routeStageUpdate,
} from './composables/useActivityCenter';
import { useConfig } from './composables/useConfig';
import { useGlobalLoading } from './composables/useGlobalLoading';
import { useHostNotificationCollector } from './composables/useHostNotificationCollector';
import { LAYER } from './ui/layers';
import { $t, i18n } from './i18n';
import { computed, provide, ref, watch } from 'vue';
import { saveUnitPaths } from './utils/saveUnit';

const { config, refreshConfig, saveConfig } = useConfig();
useDark();

const { isLoading, loadingMessage, loadingDetail } = useGlobalLoading();
const { addIfCollecting } = useHostNotificationCollector();
const sidebarWidth = ref(240);

provide('sidebarWidth', sidebarWidth);

const globalLoadingStyle = computed(() => ({
  zIndex: LAYER.globalLoading,
}));

// 设备设置对话框
const showDeviceSetupDialog = ref(false);
const currentDevice = ref<Device | null>(null);
const otherDevices = ref<Device[]>([]);
const defaultDeviceName = ref('');

const defaultUiFontFallbackStack =
  'system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Noto Sans", "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", sans-serif';

function toCssFontFamily(value: string) {
  const trimmed = value.trim();
  if (!trimmed) return '';
  if (trimmed.includes(',')) return trimmed;
  if (trimmed.includes('"') || trimmed.includes("'")) return trimmed;
  return trimmed.includes(' ') ? `"${trimmed}"` : trimmed;
}

const uiFontStack = computed(() => {
  const appearance = config.value?.settings?.appearance;
  if (!appearance?.custom_font_enabled) return null;
  const family = String(appearance?.ui_font_family ?? '');
  const cssFamily = toCssFontFamily(family);
  if (!cssFamily) return null;
  return `${cssFamily}, ${defaultUiFontFallbackStack}`;
});

function applyUiFont(stack: string | null) {
  if (typeof window === 'undefined') return;
  const style = document.documentElement.style;
  if (stack) {
    style.setProperty('--rgsm-ui-font-family', stack);
    style.setProperty('--el-font-family', stack);
  } else {
    style.removeProperty('--rgsm-ui-font-family');
    style.removeProperty('--el-font-family');
  }
}

// 检查当前设备是否已设置
async function checkDeviceSetup() {
  try {
    // 获取当前设备信息
    const result = await commands.getCurrentDeviceInfo();
    if (result.status === 'ok') {
      currentDevice.value = result.data;

      // 从配置中获取所有设备
      if (config.value && config.value.devices) {
        // 过滤掉当前设备，只显示其他设备
        otherDevices.value = Object.values(config.value.devices)
          .filter((device) => device && device.id !== currentDevice.value?.id)
          .filter((device): device is Device => device !== undefined);
      }

      // 如果当前设备不在配置中，显示设备设置对话框
      if (
        config.value &&
        (!config.value.devices || !config.value.devices[currentDevice.value.id])
      ) {
        defaultDeviceName.value = currentDevice.value.name;
        showDeviceSetupDialog.value = true;
      }
    }
  } catch (e) {
    console.error('Error checking device setup:', e);
    notifyError($t('error.get_device_info_failed'));
  }
}

// 处理设备设置确认
async function handleDeviceSetup(deviceName: string, importFromDeviceId?: string) {
  try {
    if (!config.value || !currentDevice.value) return;

    // 确保devices对象存在
    if (!config.value.devices) {
      config.value.devices = {};
    }

    // 更新当前设备信息
    const updatedDevice = {
      ...currentDevice.value,
      name: deviceName,
    };

    config.value.devices[updatedDevice.id] = updatedDevice;

    // 如果选择了导入设备，则导入路径
    if (importFromDeviceId && config.value.games) {
      const currentDeviceId = updatedDevice.id;

      // 遍历所有游戏，复制源设备的路径到当前设备
      for (const game of config.value.games) {
        // 复制存档路径
        for (const savePath of game.save_paths || []) {
          const paths = saveUnitPaths(savePath);
          if (paths) {
            if (paths[importFromDeviceId]) {
              paths[currentDeviceId] = paths[importFromDeviceId];
            }
          }
        }

        // 复制游戏启动路径
        if (game.game_paths && game.game_paths[importFromDeviceId]) {
          game.game_paths[currentDeviceId] = game.game_paths[importFromDeviceId];
        }
      }

      notifySuccess($t('device_setup.import_success'));
    }

    // 保存配置
    await saveConfig();
  } catch (e) {
    console.error('Error saving device setup:', e);
    notifyError($t('error.update_device_failed'));
  }
}

async function initializeApp() {
  try {
    await refreshConfig();
    const currentLocale = config.value.settings.locale;
    if (currentLocale) {
      i18n.global.locale.value = currentLocale as typeof i18n.global.locale.value;
    }
    applyUiFont(uiFontStack.value);
    await navigateTo(config.value!.settings.home_page ?? '/');

    // 在应用启动时检查设备设置
    await checkDeviceSetup();
  } catch {
    notifyError($t('home.wrong_homepage'));
    await navigateTo('/');
  }
}

void initializeApp();
events.ipcNotification.listen((event) => {
  const ev = event.payload;
  if (addIfCollecting(ev)) return;
  // Backend stage text enriches the running drawer entry instead of toasting.
  if (ev.level === 'info' && routeStageUpdate(ev.title, ev.msg)) return;
  switch (ev.level) {
    case 'info':
      notifyInfo(ev.title || $t('misc.info'), ev.msg);
      break;
    case 'warning':
      notifyWarning(ev.title || $t('misc.warning'), ev.msg);
      break;
    case 'error':
      notifyError(ev.title || $t('misc.error'), ev.msg);
      break;
  }
});

if (typeof window !== 'undefined') {
  watch(uiFontStack, (stack) => applyUiFont(stack), { immediate: true });
}

// 下方代码由于 tauri-specta 的bug导致无法正常运行，因此使用上方方式替代
// events.ipcNotification.listen((event) => {
//   let ev = event.payload;
//   switch (ev.level) {
//     case "info":
//       showInfo({ message: ev.msg, title: ev.title });
//       break;
//     case "warning":
//       showWarning({ message: ev.msg, title: ev.title });
//       break;
//     case "error":
//       showError({ message: ev.msg, title: ev.title });
//       break;
//   }
// });
</script>

<template>
  <div>
    <div class="app-shell">
      <aside class="app-aside" :style="{ width: sidebarWidth + 'px' }">
        <MainSideBar />
      </aside>
      <main class="app-main">
        <RouterView v-slot="{ Component }">
          <Transition name="page" mode="out-in">
            <component :is="Component" />
          </Transition>
        </RouterView>
      </main>
    </div>

    <!-- 设备设置对话框 -->
    <DeviceSetupDialog
      v-model="showDeviceSetupDialog"
      :default-device-name="defaultDeviceName"
      :other-devices="otherDevices"
      @confirm="handleDeviceSetup"
    />

    <Transition name="global-loading-fade">
      <div v-if="isLoading" class="global-loading-overlay" :style="globalLoadingStyle">
        <div class="global-loading-card">
          <LoaderCircle class="global-loading-spinner" :size="36" />
          <p class="global-loading-text">{{ loadingMessage }}</p>
          <p v-if="loadingDetail" class="global-loading-detail">{{ loadingDetail }}</p>
        </div>
      </div>
    </Transition>

    <ActivityDrawer />
    <KToaster />
    <KFeedbackHost />
  </div>
</template>

<style>
html,
body {
  margin: 0;
  overflow: hidden;
}

.app-shell {
  display: flex;
  height: 100vh;
  overflow: hidden;
}

.app-aside {
  flex-shrink: 0;
  height: 100%;
  overflow: hidden;
}

.app-main {
  flex: 1;
  min-width: 0;
  height: 100%;
  overflow-x: hidden;
  overflow-y: auto;
  /* 与旧 el-main 默认内边距保持一致:未迁移页面仍按 20px 布局;
     主页等整页画面用负 margin 抵消 */
  padding: 20px;
}

/* Custom font family - applied globally when user enables custom font */
body,
button,
input,
select,
textarea,
.el-button,
.el-input__inner,
.el-select,
.el-menu,
.el-menu-item,
.el-tabs__item,
.el-dialog,
.el-message-box,
.el-notification,
.el-table,
.el-form-item__label,
.el-checkbox__label,
.el-radio__label,
.el-alert,
.el-tag,
[class^='el-'] {
  font-family: var(--rgsm-ui-font-family, var(--font-sans-stack)) !important;
}

.global-loading-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.45);
  display: flex;
  align-items: center;
  justify-content: center;
  backdrop-filter: blur(2px);
}

.global-loading-card {
  min-width: 260px;
  padding: 1.75rem 2.5rem;
  border-radius: var(--radius-md);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 1rem;
  background: var(--surface);
  box-shadow: var(--shadow-overlay);
  color: var(--text);
  text-align: center;
}

.global-loading-spinner {
  animation: global-loading-spin 1s linear infinite;
}

.global-loading-text {
  margin: 0;
  font-size: 1rem;
  line-height: 1.4;
}

.global-loading-detail {
  margin: 0.25rem 0 0;
  font-size: 0.8rem;
  line-height: 1.3;
  color: var(--text-dim);
  opacity: 0.85;
}

@keyframes global-loading-spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

.global-loading-fade-enter-active,
.global-loading-fade-leave-active {
  transition: opacity 0.2s ease;
}

.global-loading-fade-enter-from,
.global-loading-fade-leave-to {
  opacity: 0;
}

.page-enter-active,
.page-leave-active {
  transition: all 0.2s ease-out;
}

.page-enter-from,
.page-leave-to {
  opacity: 0.4;
  filter: blur(0.2rem);
}
</style>
