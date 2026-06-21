<script setup lang="ts">
import 'element-plus/theme-chalk/dark/css-vars.css';

import { listen } from '@tauri-apps/api/event';
import { Loading } from '@element-plus/icons-vue';
import { useDark } from '@vueuse/core';
import ActivityDrawer from './components/ActivityDrawer.vue';
import DeviceSetupDialog from './components/DeviceSetupDialog.vue';
import { commands } from './bindings';
import type { Device } from './bindings';
import { notifyInfo, notifyWarning, notifyError } from './composables/useActivityCenter';
import { useConfig } from './composables/useConfig';
import { useGlobalLoading } from './composables/useGlobalLoading';
import { useIpcNotificationCollector } from './composables/useIpcNotificationCollector';
import { LAYER } from './ui/layers';
import { $t, i18n } from './i18n';
import { computed, provide, ref, watch } from 'vue';

const { config, refreshConfig, saveConfig } = useConfig();
useDark();

const { isLoading, loadingMessage, loadingDetail } = useGlobalLoading();
const { addIfCollecting } = useIpcNotificationCollector();
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
          if (savePath.paths) {
            if (savePath.paths[importFromDeviceId]) {
              savePath.paths[currentDeviceId] = savePath.paths[importFromDeviceId];
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
type NotificationPayload = {
  level: 'info' | 'warning' | 'error';
  msg: string;
  title?: string;
};

listen<NotificationPayload>('Notification', (event) => {
  const ev = event.payload;
  if (addIfCollecting(ev)) return;
  switch (ev.level.toLowerCase()) {
    case 'info':
      notifyInfo(ev.title ?? $t('misc.info'), ev.msg);
      break;
    case 'warning':
      notifyWarning(ev.title ?? $t('misc.warning'), ev.msg);
      break;
    case 'error':
      notifyError(ev.title ?? $t('misc.error'), ev.msg);
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
    <ElContainer class="app-shell">
      <ElAside :width="sidebarWidth + 'px'">
        <MainSideBar />
      </ElAside>
      <ElScrollbar>
        <ElMain>
          <RouterView v-slot="{ Component }">
            <Transition name="page" mode="out-in">
              <component :is="Component" />
            </Transition>
          </RouterView>
        </ElMain>
      </ElScrollbar>
    </ElContainer>

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
          <el-icon class="global-loading-spinner" :size="36">
            <Loading />
          </el-icon>
          <p class="global-loading-text">{{ loadingMessage }}</p>
          <p v-if="loadingDetail" class="global-loading-detail">{{ loadingDetail }}</p>
        </div>
      </div>
    </Transition>

    <ActivityDrawer />
  </div>
</template>

<style>
html,
body {
  margin: 0;
  overflow: hidden;
}

.app-shell {
  height: 100vh;
  overflow: hidden;
}

.app-shell .el-aside {
  height: 100%;
  overflow: hidden;
}

.app-shell .el-scrollbar {
  height: 100%;
  width: 100%;
  overflow: hidden;
}

/* Custom font family - applied globally when user enables custom font */
:root {
  --rgsm-ui-font-family: var(--el-font-family);
}

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
  font-family: var(--rgsm-ui-font-family) !important;
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
  border-radius: 1rem;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 1rem;
  background: var(--el-bg-color-overlay);
  box-shadow: 0 20px 40px rgba(0, 0, 0, 0.25);
  color: var(--el-text-color-primary);
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
  color: var(--el-text-color-secondary);
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
