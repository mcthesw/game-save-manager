<script lang="ts" setup>
// TODO:调整日志设置，比如删除日
import { computed, ref, watch, onMounted } from 'vue';
import { $t, i18n } from '../i18n';
import { ElOption } from 'element-plus';
import draggable from 'vuedraggable';
import {
  Setting,
  Document,
  Unlock,
  Moon,
  Tools,
} from '@element-plus/icons-vue';
import HotkeySelector from '../components/HotkeySelector.vue';
import { useNavigationLinks } from '../composables/useNavigationLinks';
import { useDark, useDebounceFn } from '@vueuse/core';
import { commands } from '~/bindings';
import type {
  QuickActionSoundPreferences,
  QuickActionSoundSlots,
  QuickActionSoundSource,
  QuickActionsSettings,
  LudusaviManifestStatus,
} from '~/bindings';
import { error, info } from '@tauri-apps/plugin-log';
import type { Device } from '../bindings';

const isDark = useDark();
const { config, refreshConfig, saveConfig } = useConfig();
const { showSuccess, showError, showInfo } = useNotification();
const feedback = useFeedback();
const locale_message = i18n.global.messages;
const locale_names = i18n.global.availableLocales;
const activeTab = ref('general');
const hotkeysChanged = ref(false);
const gameOrderChanged = ref(false);
const { withLoading } = useGlobalLoading();
type SoundModeOption = 'default' | 'file';
let skipQuickActionChange = true;

// System fonts loaded from backend
const systemFonts = ref<string[]>([]);
const systemFontsLoading = ref(false);

// Fallback font suggestions when system fonts are not yet loaded
const fallbackFontSuggestions = [
  'Segoe UI',
  'Microsoft YaHei',
  'PingFang SC',
  'Hiragino Sans GB',
  'Noto Sans',
  'Noto Sans CJK SC',
  'Inter',
  'Roboto',
  'Helvetica Neue',
  'Arial',
  'system-ui',
];

// Use system fonts if available, otherwise fallback to suggestions
const fontOptions = computed(() =>
  systemFonts.value.length > 0 ? systemFonts.value : fallbackFontSuggestions
);

async function fetchSystemFonts() {
  try {
    systemFontsLoading.value = true;
    systemFonts.value = await commands.getSystemFonts();
  } catch (e) {
    error(`Error fetching system fonts: ${e}`);
    // Silently fail, fallback will be used
  } finally {
    systemFontsLoading.value = false;
  }
}

// 设备管理相关
const currentDevice = ref<Device>({ id: '', name: '' });
const otherDevices = ref<Device[]>([]);

// Ludusavi manifest management
const ludusaviManifest = ref<LudusaviManifestStatus | null>(null);
const ludusaviManifestLoading = ref(false);
const ludusaviManifestUpdating = ref(false);
const ludusaviManifestResetting = ref(false);

function formatManifestSource(source?: string) {
  if (source === 'local') return $t('settings.manifest_source_local');
  if (source === 'bundled') return $t('settings.manifest_source_bundled');
  return source || '-';
}

function formatManifestEtag(etag?: string | null) {
  if (!etag) return '-';
  // `W/"<hash>"` -> `<hash>` (trim for readability, best-effort)
  const trimmed = etag.replace(/^W\//, '').replaceAll('"', '');
  return trimmed.length > 16 ? `${trimmed.slice(0, 16)}...` : trimmed;
}

async function refreshLudusaviManifestStatus() {
  try {
    ludusaviManifestLoading.value = true;
    const result = await commands.getLudusaviManifestStatus();
    if (result.status === 'ok') {
      ludusaviManifest.value = result.data;
    } else {
      showError({ message: result.error });
    }
  } catch (e) {
    error(`Error getting ludusavi manifest status: ${e}`);
    showError({ message: $t('settings.manifest_fetch_failed') });
  } finally {
    ludusaviManifestLoading.value = false;
  }
}

async function updateLudusaviManifest() {
  try {
    ludusaviManifestUpdating.value = true;
    const result = await commands.updateLudusaviManifest();
    if (result.status === 'ok') {
      ludusaviManifest.value = result.data;
      showSuccess({ message: $t('settings.manifest_update_success') });
    } else {
      showError({ message: result.error });
    }
  } catch (e) {
    error(`Error updating ludusavi manifest: ${e}`);
    showError({ message: $t('settings.manifest_update_failed') });
  } finally {
    ludusaviManifestUpdating.value = false;
  }
}

async function resetLudusaviManifest() {
  const hadLocal = Boolean(ludusaviManifest.value?.hasLocal);
  try {
    ludusaviManifestResetting.value = true;
    const result = await commands.resetLudusaviManifestToBundled();
    if (result.status === 'ok') {
      ludusaviManifest.value = result.data;
      if (hadLocal) {
        showSuccess({ message: $t('settings.manifest_reset_success') });
      } else {
        showInfo({ message: $t('settings.manifest_already_bundled') });
      }
    } else {
      showError({ message: result.error });
    }
  } catch (e) {
    error(`Error resetting ludusavi manifest: ${e}`);
    showError({ message: $t('settings.manifest_reset_failed') });
  } finally {
    ludusaviManifestResetting.value = false;
  }
}

// 使用debounce来合并多次保存操作
const debouncedSaveConfig = useDebounceFn(async () => {
  try {
    await saveConfig();
  } catch (e) {
    error(`save config error: ${e}`);
    showError({ message: $t('error.set_config_failed') });
  }
}, 500);

async function load_config() {
  skipQuickActionChange = true;
  await refreshConfig();
  ensureQuickActionDefaults();
  await fetchDeviceInfo();
}

async function reset_settings() {
  try {
    await commands.resetSettings();
    showSuccess({ message: $t('settings.reset_success') });
    load_config();
  } catch (e) {
    error(`reset settings error: ${e}`);
    showError({ message: $t('error.reset_settings_failed') });
  }
}

async function backup_all() {
  try {
    await feedback.prompt($t('settings.backup_all_hint'), $t('home.hint'), {
      confirmButtonText: $t('settings.confirm'),
      cancelButtonText: $t('settings.cancel'),
      inputPattern: /yes/,
      inputErrorMessage: $t('settings.invalid_input_error'),
    });

    try {
      await withLoading(async () => {
        await commands.backupAll();
      }, $t('settings.backup_all_in_progress'));
      showSuccess({ message: $t('settings.success') });
    } catch (e) {
      error(`backup all error: ${e}`);
      showError({ message: $t('settings.failed') });
    }
  } catch {
    showInfo({ message: $t('settings.operation_canceled') });
  }
}

async function apply_all() {
  try {
    await feedback.prompt($t('settings.apply_all_hint'), $t('home.hint'), {
      confirmButtonText: $t('settings.confirm'),
      cancelButtonText: $t('settings.cancel'),
      inputPattern: /yes/,
      inputErrorMessage: $t('settings.invalid_input_error'),
    });
    await withLoading(async () => {
      await commands.applyAll();
    }, $t('settings.apply_all_in_progress'));
  } catch (e) {
    if (e instanceof Error) {
      error(`apply all error: ${e}`);
    } else {
      showInfo({ message: $t('settings.operation_canceled') });
    }
  }
}

function open_log_folder() {
  try {
    commands.openFileOrFolder('log');
  } catch (e) {
    error(`open log folder error: ${e}`);
    showError({ message: $t('error.open_log_folder_failed') });
  }
}

// 保存快捷键设置
async function saveHotkeys() {
  try {
    await saveConfig();
    hotkeysChanged.value = false;
    // 只显示功能完成的消息，而不是保存成功
    showSuccess({ message: $t('settings.hotkeys_saved') });
  } catch (e) {
    error(`save hotkeys error: ${e}`);
    showError({ message: $t('error.set_config_failed') });
  }
}

// 保存游戏顺序设置
async function saveGameOrder() {
  try {
    await saveConfig();
    gameOrderChanged.value = false;
    // 只显示功能完成的消息，而不是保存成功
    showSuccess({ message: $t('settings.game_order_saved') });
  } catch (e) {
    error(`save game order error: ${e}`);
    showError({ message: $t('error.set_config_failed') });
  }
}

// 翻译网站
async function translate_website() {
  try {
    await commands.openUrl(
      'https://github.com/mcthesw/game-save-manager/blob/main/CONTRIBUTING.md'
    );
  } catch (e) {
    error(`open translate website error: ${e}`);
    showError({ message: $t('error.open_url_failed') });
  }
}

// 获取设备信息
async function fetchDeviceInfo() {
  try {
    // 获取当前设备信息
    const result = await commands.getCurrentDeviceInfo();
    if (result.status === 'ok') {
      currentDevice.value = result.data;

      // 从配置中获取所有设备
      if (config.value && config.value.devices) {
        // 过滤掉当前设备，只显示其他设备
        otherDevices.value = Object.values(config.value.devices)
          .filter((device) => device && device.id !== currentDevice.value.id)
          // 确保过滤后的数组不包含undefined
          .filter((device): device is Device => device !== undefined);
      }
    } else {
      showError({ message: result.error });
    }
  } catch (e) {
    error(`Error getting device info: ${e}`);
    showError({ message: $t('error.get_device_info_failed') });
  }
}

// 更新设备信息
async function updateDeviceInfo() {
  try {
    if (!config.value || !currentDevice.value) return;

    // 在配置中更新设备信息
    if (!config.value.devices) {
      config.value.devices = {};
    }

    config.value.devices[currentDevice.value.id] = { ...currentDevice.value };

    // 保存配置
    await saveConfig();
    showSuccess({ message: $t('settings.device_updated') });
    await fetchDeviceInfo(); // 刷新设备列表
  } catch (e) {
    error(`Error updating device info: ${e}`);
    showError({ message: $t('error.update_device_failed') });
  }
}

// 从其他设备导入路径
async function importFromDevice(deviceId: string) {
  try {
    await feedback.confirm($t('settings.import_paths_confirm'), $t('settings.import_paths_title'), {
      confirmButtonText: $t('settings.confirm'),
      cancelButtonText: $t('settings.cancel'),
      type: 'warning',
    });

    // 获取当前设备ID
    const currentDeviceId = currentDevice.value?.id;
    if (!currentDeviceId || !config.value || !config.value.games) {
      throw new Error('Current device or config not available');
    }

    if (currentDeviceId === deviceId) {
      throw new Error('Cannot import from the same device');
    }

    // 遍历所有游戏，复制源设备的路径到当前设备
    for (const game of config.value.games) {
      // 复制存档路径
      for (const savePath of game.save_paths || []) {
        if (savePath.paths) {
          if (savePath.paths[deviceId]) {
            savePath.paths[currentDeviceId] = savePath.paths[deviceId];
          }
        }
      }

      // 复制游戏启动路径
      if (game.game_paths && game.game_paths[deviceId]) {
        game.game_paths[currentDeviceId] = game.game_paths[deviceId];
      }
    }

    // 保存配置
    await saveConfig();
    showSuccess({ message: $t('settings.import_paths_success') });
  } catch (e) {
    if (e instanceof Error) {
      error(`Error importing paths: ${e}`);
      showError({ message: $t('error.import_paths_failed') });
    } else {
      // 用户取消操作
      showInfo({ message: $t('settings.operation_canceled') });
    }
  }
}

function ensureQuickActionDefaults() {
  if (!config.value?.quick_action) {
    return;
  }
  const settings = config.value.quick_action as QuickActionsSettings;
  if (settings.enable_sound === undefined) {
    settings.enable_sound = true;
  }
  if (settings.enable_notification === undefined) {
    settings.enable_notification = true;
  }
  if (!settings.sounds) {
    settings.sounds = {
      success: { kind: 'default' },
      failure: { kind: 'default' },
    };
  }
}

function ensureSoundSlots(): QuickActionSoundSlots | undefined {
  ensureQuickActionDefaults();
  return config.value?.quick_action?.sounds as QuickActionSoundSlots | undefined;
}

function isFileSource(
  source: QuickActionSoundSource | undefined
): source is QuickActionSoundSource & { kind: 'file'; path: string } {
  return source?.kind === 'file';
}

function cloneSoundSource(source: QuickActionSoundSource | undefined): QuickActionSoundSource {
  if (isFileSource(source)) {
    return { kind: 'file', path: source.path ?? '' };
  }
  return { kind: 'default' };
}

function buildSoundPreferences(): QuickActionSoundPreferences | undefined {
  if (!config.value?.quick_action) {
    return undefined;
  }
  const slots = ensureSoundSlots();
  if (!slots) {
    return undefined;
  }
  return {
    enable_sound: config.value.quick_action!.enable_sound ?? true,
    sounds: {
      success: cloneSoundSource(slots.success),
      failure: cloneSoundSource(slots.failure),
    },
  };
}

const successSoundMode = computed<SoundModeOption>({
  get: () => (isFileSource(config.value?.quick_action?.sounds?.success) ? 'file' : 'default'),
  set: (mode) => {
    const slots = ensureSoundSlots();
    if (!slots) return;
    if (mode === 'default') {
      slots.success = { kind: 'default' };
    } else {
      const current = slots.success;
      const existingPath = isFileSource(current) ? (current.path ?? '') : '';
      slots.success = { kind: 'file', path: existingPath };
    }
  },
});

const failureSoundMode = computed<SoundModeOption>({
  get: () => (isFileSource(config.value?.quick_action?.sounds?.failure) ? 'file' : 'default'),
  set: (mode) => {
    const slots = ensureSoundSlots();
    if (!slots) return;
    if (mode === 'default') {
      slots.failure = { kind: 'default' };
    } else {
      const current = slots.failure;
      const existingPath = isFileSource(current) ? (current.path ?? '') : '';
      slots.failure = { kind: 'file', path: existingPath };
    }
  },
});

const successSoundPath = computed<string>({
  get: () => {
    const source = config.value?.quick_action?.sounds?.success;
    return isFileSource(source) ? (source.path ?? '') : '';
  },
  set: (value) => {
    const slots = ensureSoundSlots();
    if (!slots) return;
    slots.success = { kind: 'file', path: value };
  },
});

const failureSoundPath = computed<string>({
  get: () => {
    const source = config.value?.quick_action?.sounds?.failure;
    return isFileSource(source) ? (source.path ?? '') : '';
  },
  set: (value) => {
    const slots = ensureSoundSlots();
    if (!slots) return;
    slots.failure = { kind: 'file', path: value };
  },
});

async function togglePreview(effect: 'success' | 'failure') {
  try {
    const preferences = buildSoundPreferences();
    if (!preferences) return;
    await commands.toggleQuickActionSoundPreview(
      preferences,
      effect === 'success' ? 'Success' : 'Failure'
    );
  } catch (e) {
    error(`toggle preview error: ${e}`);
    showError({ message: $t('error.preview_sound_failed') });
  }
}

async function chooseSoundFile(target: 'success' | 'failure') {
  try {
    const path = await commands.chooseQuickActionSoundFile();
    const slots = ensureSoundSlots();
    if (!slots) return;
    if (path.status === 'ok') {
      const file_path = path.data;
      if (target === 'success') {
        slots.success = { kind: 'file', path: file_path };
      } else {
        slots.failure = { kind: 'file', path: file_path };
      }
    }
  } catch (e) {
    error(`choose sound file error: ${e}`);
    showError({ message: $t('error.choose_sound_file_error') });
  }
}

// 删除设备
async function deleteDevice(deviceId: string) {
  if (!config.value || !config.value.devices) {
    showError({ message: $t('settings.delete_device_failed') });
    return;
  }

  if (currentDevice.value?.id === deviceId) {
    showError({ message: $t('settings.delete_device_failed') });
    return;
  }

  const targetDevice = config.value.devices[deviceId];
  if (!targetDevice) {
    showError({ message: $t('settings.delete_device_failed') });
    return;
  }

  try {
    await feedback.confirm(
      `${$t('settings.delete_device_confirm_message')}

${$t('settings.device_name')}: ${targetDevice.name || deviceId}`,
      $t('settings.delete_device_confirm_title'),
      {
        confirmButtonText: $t('settings.confirm'),
        cancelButtonText: $t('settings.cancel'),
        type: 'warning',
      }
    );
  } catch {
    showInfo({ message: $t('settings.operation_canceled') });
    return;
  }

  try {
    Reflect.deleteProperty(config.value.devices, deviceId);

    if (Array.isArray(config.value.games)) {
      for (const game of config.value.games) {
        if (game.game_paths && deviceId in game.game_paths) {
          Reflect.deleteProperty(game.game_paths, deviceId);
        }

        for (const saveUnit of game.save_paths || []) {
          if (saveUnit.paths && deviceId in saveUnit.paths) {
            Reflect.deleteProperty(saveUnit.paths, deviceId);
          }
        }
      }
    }

    await saveConfig();
    showSuccess({ message: $t('settings.delete_device_success') });
    await fetchDeviceInfo();
  } catch (e) {
    error(`Error deleting device ${deviceId}: ${e}`);
    await refreshConfig();
    showError({ message: $t('settings.delete_device_failed') });
  }
}

// 监听快捷操作相关设置变更
watch(
  () => config.value.quick_action,
  () => {
    ensureQuickActionDefaults();
    if (skipQuickActionChange) {
      skipQuickActionChange = false;
      return;
    }
    hotkeysChanged.value = true;
  },
  { deep: true }
);

// 监听游戏顺序变更
watch(
  () => config.value.games,
  () => {
    gameOrderChanged.value = true;
  },
  { deep: true }
);

// 页面加载时获取设备信息
onMounted(async () => {
  await fetchDeviceInfo();
  await refreshLudusaviManifestStatus();
  fetchSystemFonts(); // Load in background, no await needed
});

watch(
  () => config.value.settings.locale,
  (new_locale) => {
    info(`locale changed to ${new_locale}`);
    if (new_locale) {
      i18n.global.locale.value = new_locale as typeof i18n.global.locale.value;
    }
    showInfo({ message: $t('settings.locale_changed') });
  }
);

watch(
  () => config.value?.settings,
  async () => {
    debouncedSaveConfig();
  },
  { deep: true } // 深度监听对象变化
);

const { linksWithGames: router_list } = useNavigationLinks();
</script>

<template>
  <el-container class="setting" direction="vertical">
    <el-card>
      <h1>{{ $t('settings.customizable_settings') }}</h1>
      <div class="button-bar">
        <el-button @click="open_log_folder()">{{ $t('settings.open_log_folder') }}</el-button>
        <el-popconfirm :title="$t('settings.confirm_reset')" :on-confirm="reset_settings">
          <template #reference>
            <el-button type="danger">{{ $t('settings.reset_settings') }}</el-button>
          </template>
        </el-popconfirm>
        <el-button type="danger" @click="backup_all">
          {{ $t('settings.backup_all') }}
        </el-button>
        <el-button type="danger" @click="apply_all">
          {{ $t('settings.apply_all') }}
        </el-button>
      </div>

      <el-tabs v-model="activeTab" type="border-card" class="settings-tabs">
        <!-- 通用设置 -->
        <el-tab-pane :label="$t('settings.general')" name="general">
          <el-divider content-position="left">
            <el-icon>
              <Setting />
            </el-icon>
            <span class="tab-title">{{ $t('settings.general') }}</span>
          </el-divider>

          <div class="setting-box">
            <ElSelect v-model="config.settings.locale">
              <ElOption
                v-for="locale in locale_names"
                :key="locale"
                :label="
                  ((locale_message as any)[locale]?.settings?.locale_name || locale) +
                  ' - ' +
                  locale
                "
                :value="locale"
              />
            </ElSelect>
            <span class="setting-label translate-website" @click="translate_website"
              >🌍 Languages - Click me to translate!</span
            >
          </div>
          <div class="setting-box">
            <ElSelect v-model="config.settings.home_page">
              <ElOption
                v-for="route_info in router_list"
                :key="route_info.text"
                :label="route_info.text"
                :value="route_info.link"
              >
                <div class="home-option-box">
                  <component :is="route_info.icon" class="home-box-icon" />
                  {{ route_info.text }}
                </div>
              </ElOption>
            </ElSelect>
            <span class="setting-label">🏠 {{ $t('settings.homepage') }}</span>
          </div>
          <div class="setting-box">
            <ElSwitch v-model="config.settings.exit_to_tray" />
            <span class="setting-label">{{ $t('settings.exit_to_tray') }}*</span>
          </div>
          <div class="setting-box">
            <ElSwitch v-model="config.settings.log_to_file" />
            <span class="setting-label">{{ $t('settings.log_to_file') }}*</span>
          </div>
          <div class="setting-box">
            <ElSwitch v-model="isDark" />
            <span class="setting-label">{{ $t('settings.enable_dark_mode') }}</span>
          </div>

          <el-divider content-position="left">
            <el-icon>
              <Document />
            </el-icon>
            <span class="tab-title">{{ $t('settings.ludusavi_manifest') }}</span>
          </el-divider>

          <div v-loading="ludusaviManifestLoading" class="manifest-box">
            <el-alert type="info" :closable="false" class="manifest-hint">
              {{ $t('settings.ludusavi_manifest_hint') }}
            </el-alert>
            <el-descriptions :column="1" size="small" border>
              <el-descriptions-item :label="$t('settings.manifest_source')">
                {{ formatManifestSource(ludusaviManifest?.source) }}
              </el-descriptions-item>
              <el-descriptions-item :label="$t('settings.manifest_updated_at')">
                {{ ludusaviManifest?.updatedAt || '-' }}
              </el-descriptions-item>
              <el-descriptions-item :label="$t('settings.manifest_etag')">
                {{ formatManifestEtag(ludusaviManifest?.etag) }}
              </el-descriptions-item>
            </el-descriptions>
            <div class="manifest-actions">
              <div class="manifest-actions-left">
                <el-button @click="refreshLudusaviManifestStatus">
                  {{ $t('settings.manifest_refresh') }}
                </el-button>
              </div>
              <div class="manifest-actions-right">
                <el-button
                  type="primary"
                  :loading="ludusaviManifestUpdating"
                  @click="updateLudusaviManifest"
                >
                  {{ $t('settings.manifest_update') }}
                </el-button>
                <el-button
                  type="danger"
                  plain
                  :loading="ludusaviManifestResetting"
                  @click="resetLudusaviManifest"
                >
                  {{ $t('settings.manifest_reset') }}
                </el-button>
              </div>
            </div>
          </div>
        </el-tab-pane>

        <!-- 备份设置 -->
        <el-tab-pane :label="$t('settings.backup_settings')" name="backup">
          <el-divider content-position="left">
            <el-icon>
              <Document />
            </el-icon>
            <span class="tab-title">{{ $t('settings.backup_settings') }}</span>
          </el-divider>

          <div class="setting-box">
            <ElSwitch v-model="config.settings.prompt_when_not_described" />
            <span class="setting-label">{{ $t('settings.prompt_when_not_described') }}</span>
          </div>
          <div class="setting-box">
            <ElSwitch v-model="config.settings.prompt_when_auto_backup" />
            <span class="setting-label">{{ $t('settings.prompt_when_auto_backup') }}</span>
          </div>
          <div class="setting-box">
            <ElInputNumber
              v-model="config.settings.max_auto_backup_count"
              :min="0"
              :max="999"
              :step="1"
            />
            <span class="setting-label">{{ $t('settings.max_auto_backup_count') }}</span>
          </div>
          <div class="setting-box">
            <ElSwitch v-model="config.settings.extra_backup_when_apply" />
            <span class="setting-label">{{ $t('settings.extra_backup_when_apply') }}</span>
          </div>
          <div class="setting-box">
            <ElSwitch v-model="config.settings.default_delete_before_apply" />
            <span class="setting-label">{{ $t('settings.default_delete_before_apply') }}</span>
          </div>
          <div class="setting-box">
            <ElSwitch v-model="config.settings.add_new_to_favorites" />
            <span class="setting-label">{{ $t('settings.add_new_to_favorites') }}</span>
          </div>
        </el-tab-pane>

        <!-- 界面设置 -->
        <el-tab-pane :label="$t('settings.ui_settings')" name="ui">
          <el-divider content-position="left">
            <el-icon>
              <Moon />
            </el-icon>
            <span class="tab-title">{{ $t('settings.ui_settings') }}</span>
          </el-divider>

          <div class="setting-box">
            <ElSelect v-model="config.settings.save_list_expand_behavior">
              <ElOption
                :label="$t('settings.save_list_expand_behavior_default_open')"
                value="always_open"
              />
              <ElOption
                :label="$t('settings.save_list_expand_behavior_default_closed')"
                value="always_closed"
              />
              <ElOption
                :label="$t('settings.save_list_expand_behavior_remember_last')"
                value="remember_last"
              />
            </ElSelect>
            <span class="setting-label">{{ $t('settings.save_list_expand_behavior') }}</span>
          </div>
          <div class="setting-box">
            <ElSwitch v-model="config.settings.default_expend_favorites_tree" />
            <span class="setting-label">{{ $t('settings.default_expend_favorites_tree') }}</span>
          </div>
        </el-tab-pane>

        <!-- 外观设置 -->
        <el-tab-pane :label="$t('settings.appearance_settings')" name="appearance">
          <el-divider content-position="left">
            <el-icon>
              <Moon />
            </el-icon>
            <span class="tab-title">{{ $t('settings.appearance_settings') }}</span>
          </el-divider>

          <div class="setting-box">
            <ElSwitch v-model="config.settings.appearance!.custom_font_enabled" />
            <span class="setting-label">{{ $t('settings.custom_font_enabled') }}</span>
          </div>
          <div class="setting-box">
            <ElSelect
              v-model="config.settings.appearance!.ui_font_family"
              class="font-select"
              filterable
              allow-create
              default-first-option
              clearable
              :loading="systemFontsLoading"
              :disabled="!config.settings.appearance?.custom_font_enabled"
              :placeholder="$t('settings.ui_font_family_placeholder')"
            >
              <ElOption v-for="font in fontOptions" :key="font" :label="font" :value="font" />
            </ElSelect>
            <span class="setting-label">{{ $t('settings.ui_font_family') }}</span>
          </div>
          <el-alert type="info" :closable="false" class="manifest-hint">
            {{ $t('settings.custom_font_hint') }}
          </el-alert>
        </el-tab-pane>

        <!-- 设备管理 -->
        <el-tab-pane :label="$t('settings.device_settings')" name="device">
          <el-divider content-position="left">
            <el-icon>
              <Tools />
            </el-icon>
            <span class="tab-title">{{ $t('settings.device_settings') }}</span>
          </el-divider>

          <!-- 当前设备信息 -->
          <div class="setting-box">
            <h3>{{ $t('settings.current_device') }}</h3>
            <div class="device-info">
              <el-form :model="currentDevice" label-position="top">
                <el-form-item :label="$t('settings.device_name')">
                  <el-input v-model="currentDevice.name" @change="updateDeviceInfo" />
                </el-form-item>
                <el-form-item :label="$t('settings.device_id')">
                  <el-input v-model="currentDevice.id" disabled />
                </el-form-item>
              </el-form>
            </div>
          </div>

          <!-- 其他设备列表 -->
          <div class="setting-box">
            <h3>{{ $t('settings.other_devices') }}</h3>
            <el-table :data="otherDevices" style="width: 100%">
              <el-table-column prop="name" :label="$t('settings.device_name')" />
              <el-table-column prop="id" :label="$t('settings.device_id')" width="220" />
              <el-table-column :label="$t('settings.actions')" width="220">
                <template #default="scope">
                  <el-button type="primary" size="small" @click="importFromDevice(scope.row.id)">
                    {{ $t('settings.import_paths') }}
                  </el-button>
                  <el-button type="danger" size="small" plain @click="deleteDevice(scope.row.id)">
                    {{ $t('settings.delete_device') }}
                  </el-button>
                </template>
              </el-table-column>
            </el-table>
          </div>
        </el-tab-pane>

        <!-- 快捷键设置 -->
        <el-tab-pane :label="$t('settings.hotkey_settings')" name="hotkeys">
          <el-divider content-position="left">
            <el-icon>
              <Unlock />
            </el-icon>
            <span class="tab-title">{{ $t('settings.hotkey_settings') }}</span>
          </el-divider>

          <div class="setting-box">
            <div>
              <strong v-if="config.quick_action!.quick_action_game">
                {{ $t('setting.current_quick_action_game') }} :
                {{ config.quick_action!.quick_action_game?.name }}
              </strong>
            </div>
            <div class="quick-action-row">
              <ElSwitch v-model="config.quick_action!.enable_sound" />
              <span class="setting-label">{{ $t('settings.quick_action_enable_sound') }}</span>
            </div>
            <div class="quick-action-row">
              <ElSwitch v-model="config.quick_action!.enable_notification" />
              <span class="setting-label">{{
                $t('settings.quick_action_enable_notification')
              }}</span>
            </div>
            <div class="sound-setting">
              <h3>{{ $t('settings.quick_action_sound_title') }}</h3>
              <div class="sound-row">
                <span class="sound-label">{{ $t('settings.quick_action_sound_success') }}</span>
                <ElSelect v-model="successSoundMode" class="sound-mode-select">
                  <ElOption
                    :label="$t('settings.quick_action_sound_mode_default')"
                    value="default"
                  />
                  <ElOption :label="$t('settings.quick_action_sound_mode_custom')" value="file" />
                </ElSelect>
                <template v-if="successSoundMode === 'file'">
                  <ElInput
                    v-model="successSoundPath"
                    class="sound-path-input"
                    :placeholder="$t('settings.quick_action_sound_file_placeholder')"
                  />
                  <ElButton @click="chooseSoundFile('success')">
                    {{ $t('settings.quick_action_sound_choose') }}
                  </ElButton>
                </template>
                <ElButton class="sound-preview-button" @click="togglePreview('success')">
                  {{ $t('settings.quick_action_sound_preview_button') }}
                </ElButton>
              </div>
              <div class="sound-row">
                <span class="sound-label">{{ $t('settings.quick_action_sound_failure') }}</span>
                <ElSelect v-model="failureSoundMode" class="sound-mode-select">
                  <ElOption
                    :label="$t('settings.quick_action_sound_mode_default')"
                    value="default"
                  />
                  <ElOption :label="$t('settings.quick_action_sound_mode_custom')" value="file" />
                </ElSelect>
                <template v-if="failureSoundMode === 'file'">
                  <ElInput
                    v-model="failureSoundPath"
                    class="sound-path-input"
                    :placeholder="$t('settings.quick_action_sound_file_placeholder')"
                  />
                  <ElButton @click="chooseSoundFile('failure')">
                    {{ $t('settings.quick_action_sound_choose') }}
                  </ElButton>
                </template>
                <ElButton class="sound-preview-button" @click="togglePreview('failure')">
                  {{ $t('settings.quick_action_sound_preview_button') }}
                </ElButton>
              </div>
            </div>
            <HotkeySelector v-model="config.quick_action!.hotkeys" />
            <div class="setting-action">
              <el-button type="primary" :disabled="!hotkeysChanged" @click="saveHotkeys">
                {{ $t('settings.save_hotkeys') }}
              </el-button>
              <el-tag v-if="hotkeysChanged" type="warning">{{
                $t('settings.unsaved_changes')
              }}</el-tag>
            </div>
          </div>
        </el-tab-pane>

        <!-- 游戏排序 -->
        <el-tab-pane :label="$t('settings.game_order')" name="gameOrder">
          <el-divider content-position="left">
            <el-icon>
              <Tools />
            </el-icon>
            <span class="tab-title">{{ $t('settings.game_order') }}</span>
          </el-divider>

          <div class="setting-box drag-game-box">
            <!-- 移除handle属性，恢复原有的拖拽功能 -->
            <draggable v-model="config.games" item-key="name" :force-fallback="true">
              <template #item="{ element }">
                <div class="game-order-box">
                  {{ element.name }}
                </div>
              </template>
            </draggable>
            <div class="setting-action">
              <el-button type="primary" :disabled="!gameOrderChanged" @click="saveGameOrder">
                {{ $t('settings.save_game_order') }}
              </el-button>
              <el-tag v-if="gameOrderChanged" type="warning">{{
                $t('settings.unsaved_changes')
              }}</el-tag>
            </div>
          </div>
        </el-tab-pane>
      </el-tabs>
    </el-card>
  </el-container>
</template>

<style scoped>
.el-button {
  margin-right: 10px;
  margin-top: 5px;
}

.el-button + .el-button {
  margin-left: 0 !important;
}

.el-card {
  overflow-y: auto;
  height: 100%;
}

.el-switch {
  margin-right: 20px;
}

.setting-box {
  margin-top: 15px;
  padding: 10px;
  border-radius: 4px;
  transition: background-color 0.3s;
}

.setting-box:hover {
  background-color: var(--el-fill-color-light);
}

.manifest-box {
  margin-top: 10px;
  padding: 10px;
  border-radius: 4px;
  background-color: var(--el-fill-color-light);
}

.manifest-actions {
  margin-top: 12px;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px;
}

.manifest-actions-left,
.manifest-actions-right {
  display: flex;
  align-items: center;
  gap: 10px;
}

.manifest-actions-right {
  margin-left: auto;
}

.manifest-actions :deep(.el-button) {
  margin: 0;
}

.manifest-hint {
  margin-bottom: 12px;
}

.manifest-box :deep(.el-descriptions__table) {
  table-layout: auto;
}

.manifest-box :deep(.el-descriptions__label) {
  width: 1%;
  white-space: nowrap;
}

.setting-label {
  margin-left: 10px;
  vertical-align: middle;
}

.quick-action-row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 10px;
}

.sound-setting {
  margin-top: 15px;
  padding: 10px;
  border-radius: 4px;
  background-color: var(--el-fill-color-light);
}

.sound-setting h3 {
  margin: 0 0 10px 0;
}

.sound-row {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px;
  margin-top: 10px;
}

.sound-label {
  min-width: 120px;
  font-weight: 500;
}

.sound-mode-select {
  width: 160px;
}

.sound-path-input {
  flex: 1;
  min-width: 220px;
}

.sound-preview-button {
  white-space: nowrap;
}

.setting-action {
  margin-top: 15px;
  display: flex;
  align-items: center;
  gap: 10px;
}

.tab-title {
  margin-left: 8px;
  font-weight: 600;
}

/** 以下是排序盒子样式 */
.game-order-box {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: medium;
  margin-top: 10px;
  padding: 10px;
  cursor: move;
  /* 更改游戏排序盒子的光标为move，提示可拖动 */
  transition: all 0.3s ease;
  border: 1px solid var(--el-border-color);
  border-radius: 4px;
}

.game-order-box:hover {
  box-shadow: var(--el-box-shadow-light);
  transform: translateY(-2px);
}

/** 以下是首页选择样式 */
.home-option-box {
  display: flex;
  align-items: center;
}

.home-box-icon {
  height: 1em;
  width: 1em;
  margin-right: 10px;
}

.drag-game-box {
  user-select: none;
}

.el-select {
  max-width: 200px;
}

.el-select.font-select {
  max-width: 360px;
}

.settings-tabs {
  margin-top: 20px;
}

.translate-website {
  cursor: pointer;
  color: var(--el-color-primary);
  text-decoration: none;
}
</style>
