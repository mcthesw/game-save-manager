<script setup lang="ts">
import 'element-plus/theme-chalk/dark/css-vars.css' // 引入暗黑主题样式
const { config, refreshConfig, saveConfig } = useConfig();
useDark();

import { events, commands } from "./bindings";
import { useNotification } from "./composables/useNotification";
import { useConfig } from "./composables/useConfig";
import { useGlobalLoading } from "./composables/useGlobalLoading";
import { $t, i18n } from "./i18n";
import { ref, onMounted } from 'vue';
import { Loading } from "@element-plus/icons-vue";
import DeviceSetupDialog from './components/DeviceSetupDialog.vue';
import type { Device } from './bindings';

const { showInfo, showWarning, showError, showSuccess } = useNotification();
const { isLoading, loadingMessage } = useGlobalLoading();

// 设备设置对话框
const showDeviceSetupDialog = ref(false);
const currentDevice = ref<Device | null>(null);
const otherDevices = ref<Device[]>([]);
const defaultDeviceName = ref('');

// 检查当前设备是否已设置
async function checkDeviceSetup() {
  try {
    // 获取当前设备信息
    const result = await commands.getCurrentDeviceInfo();
    if (result.status === "ok") {
      currentDevice.value = result.data;
      
      // 从配置中获取所有设备
      if (config.value && config.value.devices) {
        // 过滤掉当前设备，只显示其他设备
        otherDevices.value = Object.values(config.value.devices)
          .filter(device => device && device.id !== currentDevice.value?.id)
          .filter((device): device is Device => device !== undefined);
      }
      
      // 如果当前设备不在配置中，显示设备设置对话框
      if (config.value && (!config.value.devices || !config.value.devices[currentDevice.value.id])) {
        defaultDeviceName.value = currentDevice.value.name;
        showDeviceSetupDialog.value = true;
      }
    }
  } catch (e) {
    console.error('Error checking device setup:', e);
    showError({ message: $t('error.get_device_info_failed') });
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
      name: deviceName
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
      
      showSuccess({ message: $t('device_setup.import_success') });
    }
    
    // 保存配置
    await saveConfig();
  } catch (e) {
    console.error('Error saving device setup:', e);
    showError({ message: $t('error.update_device_failed') });
  }
}

try {
  await refreshConfig();
  i18n.global.locale.value = config.value.settings.locale! as any;
  await navigateTo(config.value!.settings.home_page);
  
  // 在应用启动时检查设备设置
  await checkDeviceSetup();
} catch (e) {
  showError({ message: $t("home.wrong_homepage") });
  navigateTo("/");
}


import { listen } from '@tauri-apps/api/event';
listen('Notification', (event) => {
  let ev = event.payload as any;
  switch (ev.level.toLowerCase()) {
    case "info":
      showInfo({ message: ev.msg, title: ev.title });
      break;
    case "warning":
      showWarning({ message: ev.msg, title: ev.title });
      break;
    case "error":
      showError({ message: ev.msg, title: ev.title });
      break;
  }
})

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
    <NuxtLayout>
      <NuxtPage />
    </NuxtLayout>

    <!-- 设备设置对话框 -->
    <DeviceSetupDialog
      v-model="showDeviceSetupDialog"
      :default-device-name="defaultDeviceName"
      :other-devices="otherDevices"
      @confirm="handleDeviceSetup"
    />

    <Transition name="global-loading-fade">
      <div v-if="isLoading" class="global-loading-overlay">
        <div class="global-loading-card">
          <el-icon class="global-loading-spinner" :size="36">
            <Loading />
          </el-icon>
          <p class="global-loading-text">{{ loadingMessage }}</p>
        </div>
      </div>
    </Transition>
  </div>
</template>

<style>
body {
  margin: 0px !important;
}

.global-loading-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.45);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 9999;
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