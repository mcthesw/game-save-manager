<script lang="ts" setup>
// TODO:调整日志设置，比如删除日
import { computed, nextTick, ref, watch, onMounted } from 'vue';
import { $t, i18n } from '../i18n';
import draggable from 'vuedraggable';
import {
  Setting,
  Document,
  Unlock,
  Moon,
  Tools,
  Search,
  FolderOpened,
  Close,
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
import { saveUnitPaths } from '../utils/saveUnit';

const isDark = useDark();
const { config, refreshConfig, saveConfig } = useConfig();
const feedback = useFeedback();
const locale_message = i18n.global.messages;
const locale_names = i18n.global.availableLocales;
const currentQuickActionGame = computed(() => {
  const identity = config.value.quick_action?.quick_action_game_id;
  if (!identity) return undefined;
  return config.value.games.find((game) => game.storage_key === identity || game.name === identity);
});
const activeTab = ref('general');
const hotkeysChanged = ref(false);
const gameOrderChanged = ref(false);
const { withLoading } = useGlobalLoading();
type SoundModeOption = 'default' | 'file';
let skipQuickActionChange = true;
/** Suppress dirty tracking during programmatic config refreshes. */
let suppressConfigChangeTracking = false;

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
const currentDevice = ref<Device>({ id: '', name: '', resources: [], next_resource_id: 0 });
const otherDevices = ref<Device[]>([]);

// Ludusavi manifest management
const ludusaviManifest = ref<LudusaviManifestStatus | null>(null);
const ludusaviManifestLoading = ref(false);
const ludusaviManifestUpdating = ref(false);
const ludusaviManifestResetting = ref(false);
const hasBundledManifest = computed(() => (ludusaviManifest.value?.bundledBytes ?? 0) > 0);

function formatManifestSource(source?: string) {
  if (source === 'local') return $t('settings.manifest_source_local');
  if (source === 'bundled') return $t('settings.manifest_source_bundled');
  if (source === 'none') return $t('settings.manifest_source_none');
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
      notifyError(result.error);
    }
  } catch (e) {
    error(`Error getting ludusavi manifest status: ${e}`);
    notifyError($t('settings.manifest_fetch_failed'));
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
      notifySuccess($t('settings.manifest_update_success'));
    } else {
      notifyError(result.error);
    }
  } catch (e) {
    error(`Error updating ludusavi manifest: ${e}`);
    notifyError($t('settings.manifest_update_failed'));
  } finally {
    ludusaviManifestUpdating.value = false;
  }
}

async function resetLudusaviManifest() {
  const hadLocal = Boolean(ludusaviManifest.value?.hasLocal);
  const isBundled = hasBundledManifest.value;
  try {
    ludusaviManifestResetting.value = true;
    const result = await commands.resetLudusaviManifestToBundled();
    if (result.status === 'ok') {
      ludusaviManifest.value = result.data;
      if (hadLocal) {
        notifySuccess(
          isBundled ? $t('settings.manifest_reset_success') : $t('settings.manifest_cache_cleared')
        );
      } else {
        notifyInfo(
          isBundled
            ? $t('settings.manifest_already_bundled')
            : $t('settings.manifest_already_empty')
        );
      }
    } else {
      notifyError(result.error);
    }
  } catch (e) {
    error(`Error resetting ludusavi manifest: ${e}`);
    notifyError($t('settings.manifest_reset_failed'));
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
    notifyError($t('error.set_config_failed'));
  }
}, 500);

async function load_config() {
  suppressConfigChangeTracking = true;
  skipQuickActionChange = true;
  try {
    await refreshConfig();
    ensureQuickActionDefaults();
    const currentLocale = config.value.settings.locale;
    if (currentLocale) {
      i18n.global.locale.value = currentLocale as typeof i18n.global.locale.value;
    }
    await nextTick();
  } finally {
    suppressConfigChangeTracking = false;
    skipQuickActionChange = false;
  }
  await fetchDeviceInfo();
}

async function reset_settings() {
  try {
    await commands.resetSettings();
    notifySuccess($t('settings.reset_success'));
    await load_config();
  } catch (e) {
    error(`reset settings error: ${e}`);
    notifyError($t('error.reset_settings_failed'));
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
      notifySuccess(
        config.value.settings.cloud_settings?.backend?.type !== 'Disabled' &&
          config.value.games.some((game) => game.cloud_sync_enabled !== false)
          ? $t('settings.backup_all_success_with_sync')
          : $t('settings.success')
      );
    } catch (e) {
      error(`backup all error: ${e}`);
      notifyError($t('settings.failed'));
    }
  } catch {
    notifyInfo($t('settings.operation_canceled'));
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
      notifyInfo($t('settings.operation_canceled'));
    }
  }
}

async function open_log_folder() {
  try {
    const logDirResult = await commands.getAppLogDir();
    if (logDirResult.status === 'error') {
      error(`get log dir error: ${logDirResult.error}`);
      notifyError($t('error.open_log_folder_failed'));
      return;
    }

    const result = await commands.openFileOrFolder(logDirResult.data);
    if (result.status === 'error') {
      error(`open log folder error: ${result.error}`);
      notifyError($t('error.open_log_folder_failed'));
    }
  } catch (e) {
    error(`open log folder error: ${e}`);
    notifyError($t('error.open_log_folder_failed'));
  }
}

// 保存快捷键设置
async function saveHotkeys() {
  try {
    await saveConfig();
    hotkeysChanged.value = false;
    // 只显示功能完成的消息，而不是保存成功
    notifySuccess($t('settings.hotkeys_saved'));
  } catch (e) {
    error(`save hotkeys error: ${e}`);
    notifyError($t('error.set_config_failed'));
  }
}

// 保存游戏顺序设置
async function saveGameOrder() {
  try {
    await saveConfig();
    gameOrderChanged.value = false;
    // 只显示功能完成的消息，而不是保存成功
    notifySuccess($t('settings.game_order_saved'));
  } catch (e) {
    error(`save game order error: ${e}`);
    notifyError($t('error.set_config_failed'));
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
    notifyError($t('error.open_url_failed'));
  }
}

// 获取设备信息
async function fetchDeviceInfo() {
  try {
    // 获取当前设备信息
    const result = await commands.getCurrentDeviceInfo();
    if (result.status === 'ok') {
      currentDevice.value = {
        ...result.data,
        resources: result.data.resources ?? [],
      };

      // 从配置中获取所有设备
      if (config.value && config.value.devices) {
        otherDevices.value = Object.values(config.value.devices).reduce<Device[]>(
          (list, device) => {
            if (!device || !device.id || device.id === currentDevice.value.id) {
              return list;
            }

            list.push({
              id: device.id,
              name: device.name,
              resources: device.resources ?? [],
              next_resource_id: device.next_resource_id ?? 0,
            });
            return list;
          },
          []
        );
      }
    } else {
      notifyError(result.error);
    }
  } catch (e) {
    error(`Error getting device info: ${e}`);
    notifyError($t('error.get_device_info_failed'));
  }
}

// 更新设备信息
async function persistDeviceInfo(showSuccessMessage: boolean = true) {
  try {
    if (!config.value || !currentDevice.value) return;

    // 在配置中更新设备信息
    if (!config.value.devices) {
      config.value.devices = {};
    }

    config.value.devices[currentDevice.value.id] = { ...currentDevice.value };

    // 保存配置
    await saveConfig();
    if (showSuccessMessage) {
      notifySuccess($t('settings.device_updated'));
    }
    await fetchDeviceInfo(); // 刷新设备列表
  } catch (e) {
    error(`Error updating device info: ${e}`);
    notifyError($t('error.update_device_failed'));
  }
}

async function updateDeviceInfo() {
  await persistDeviceInfo(true);
}

// 游戏根目录管理
const detectingGameRoots = ref(false);

function getCurrentGameRoots(): string[] {
  return (currentDevice.value.resources ?? [])
    .filter((resource) => resource.kind.type === 'gameRoot')
    .map((resource) => (resource.kind.type === 'gameRoot' ? resource.kind.path : ''));
}

const gameRootResources = computed(() =>
  (currentDevice.value.resources ?? []).filter((resource) => resource.kind.type === 'gameRoot')
);
const storeAccountResources = computed(() =>
  (currentDevice.value.resources ?? []).filter((resource) => resource.kind.type === 'storeAccount')
);
const installationResources = computed(() =>
  (currentDevice.value.resources ?? []).filter(
    (resource) => resource.kind.type === 'gameInstallation'
  )
);

function addGameRoot() {
  currentDevice.value.resources ??= [];
  const id = currentDevice.value.next_resource_id ?? 0;
  currentDevice.value.resources.push({
    id,
    source: 'manual',
    kind: { type: 'gameRoot', store: 'other', path: '' },
  });
  currentDevice.value.next_resource_id = id + 1;
}

let gameRootsSaveQueue = Promise.resolve();

function saveGameRoots() {
  gameRootsSaveQueue = gameRootsSaveQueue.then(() => persistDeviceInfo(false));
  return gameRootsSaveQueue;
}

async function removeGameRoot(index: number) {
  const roots = (currentDevice.value.resources ?? []).filter(
    (resource) => resource.kind.type === 'gameRoot'
  );
  const target = roots[index];
  currentDevice.value.resources = (currentDevice.value.resources ?? []).filter(
    (resource) => resource.id !== target?.id
  );
  await saveGameRoots();
}

function updateGameRoot(index: number, value: string) {
  const resource = gameRootResources.value[index];
  if (resource?.kind.type === 'gameRoot') resource.kind.path = value;
}

async function pickGameRoot(index: number) {
  try {
    const result = await commands.chooseSaveDir();
    if (result.status === 'ok' && result.data) {
      updateGameRoot(index, result.data);
      await saveGameRoots();
    }
  } catch (e) {
    error(`Error picking game root: ${e}`);
  }
}

async function autoDetectGameRoots() {
  detectingGameRoots.value = true;
  try {
    const result = await commands.detectGameRoots();
    if (result.status === 'ok') {
      const existing = new Set(getCurrentGameRoots());
      const newRoots = result.data.filter((r) => !existing.has(r));
      if (newRoots.length === 0) {
        notifyInfo($t('settings.game_roots_no_new'));
        return;
      }
      for (const path of newRoots) {
        const id = currentDevice.value.next_resource_id ?? 0;
        currentDevice.value.resources ??= [];
        currentDevice.value.resources.push({
          id,
          source: 'detected',
          kind: { type: 'gameRoot', store: 'steam', path },
        });
        currentDevice.value.next_resource_id = id + 1;
      }
      await saveGameRoots();
      notifySuccess($t('settings.game_roots_detected', { count: newRoots.length }));
    } else {
      notifyError(result.error);
    }
  } catch (e) {
    error(`Error detecting game roots: ${e}`);
    notifyError($t('settings.game_roots_detect_failed'));
  } finally {
    detectingGameRoots.value = false;
  }
}

function addStoreAccount() {
  currentDevice.value.resources ??= [];
  const id = currentDevice.value.next_resource_id ?? 0;
  currentDevice.value.resources.push({
    id,
    source: 'manual',
    kind: { type: 'storeAccount', store: 'steam', user_id: '' },
  });
  currentDevice.value.next_resource_id = id + 1;
}

async function removeStoreAccount(id: number) {
  currentDevice.value.resources = (currentDevice.value.resources ?? []).filter(
    (resource) => resource.id !== id
  );
  await persistDeviceInfo(false);
}

function addGameInstallation() {
  const firstRoot = gameRootResources.value[0];
  if (!firstRoot) {
    notifyWarning($t('settings.installation_requires_root'));
    return;
  }
  currentDevice.value.resources ??= [];
  const id = currentDevice.value.next_resource_id ?? 0;
  currentDevice.value.resources.push({
    id,
    source: 'manual',
    kind: {
      type: 'gameInstallation',
      root_id: firstRoot.id,
      store: firstRoot.kind.type === 'gameRoot' ? firstRoot.kind.store : 'other',
      install_dir: '',
      path: '',
    },
  });
  currentDevice.value.next_resource_id = id + 1;
}

async function removeGameInstallation(id: number) {
  currentDevice.value.resources = (currentDevice.value.resources ?? []).filter(
    (resource) => resource.id !== id
  );
  await persistDeviceInfo(false);
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
        const paths = saveUnitPaths(savePath);
        if (paths) {
          if (paths[deviceId]) {
            paths[currentDeviceId] = paths[deviceId];
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
    notifySuccess($t('settings.import_paths_success'));
  } catch (e) {
    if (e instanceof Error) {
      error(`Error importing paths: ${e}`);
      notifyError($t('error.import_paths_failed'));
    } else {
      // 用户取消操作
      notifyInfo($t('settings.operation_canceled'));
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
  if (settings.notify_when_unchanged === undefined) {
    settings.notify_when_unchanged = true;
  }
  if (!settings.sounds) {
    settings.sounds = {
      success: { kind: 'default' },
      failure: { kind: 'default' },
    };
  }
  if (!settings.game_automations) {
    settings.game_automations = [];
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
    notifyError($t('error.preview_sound_failed'));
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
    notifyError($t('error.choose_sound_file_error'));
  }
}

// 删除设备
async function deleteDevice(deviceId: string) {
  if (!config.value || !config.value.devices) {
    notifyError($t('settings.delete_device_failed'));
    return;
  }

  if (currentDevice.value?.id === deviceId) {
    notifyError($t('settings.delete_device_failed'));
    return;
  }

  const targetDevice = config.value.devices[deviceId];
  if (!targetDevice) {
    notifyError($t('settings.delete_device_failed'));
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
    notifyInfo($t('settings.operation_canceled'));
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
          const paths = saveUnitPaths(saveUnit);
          if (paths && deviceId in paths) {
            Reflect.deleteProperty(paths, deviceId);
          }
        }
      }
    }

    await saveConfig();
    notifySuccess($t('settings.delete_device_success'));
    await fetchDeviceInfo();
  } catch (e) {
    error(`Error deleting device ${deviceId}: ${e}`);
    await refreshConfig();
    notifyError($t('settings.delete_device_failed'));
  }
}

async function addVnScanDir() {
  try {
    const dir = await commands.chooseSaveDir();
    if (dir.status !== 'ok') {
      return;
    }

    if (!config.value.settings.vn_scan_dirs) {
      config.value.settings.vn_scan_dirs = [];
    }

    if (!config.value.settings.vn_scan_dirs.includes(dir.data)) {
      config.value.settings.vn_scan_dirs.push(dir.data);
    }
  } catch (e) {
    error(`choose scan dir error: ${e}`);
    notifyError($t('error.choose_save_dir_error'));
  }
}

function removeVnScanDir(dir: string) {
  const vnScanDirs = config.value.settings.vn_scan_dirs ?? [];
  config.value.settings.vn_scan_dirs = vnScanDirs.filter((currentDir) => currentDir !== dir);
}

function onSaveListSortModeChange(mode: string) {
  config.value.settings.save_list_sort_direction = mode === 'last_played' ? 'desc' : 'asc';
}

// 监听快捷操作相关设置变更
watch(
  () => config.value.quick_action,
  () => {
    ensureQuickActionDefaults();
    if (suppressConfigChangeTracking) {
      skipQuickActionChange = false;
      return;
    }
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
    if (suppressConfigChangeTracking) {
      return;
    }
    gameOrderChanged.value = true;
  },
  { deep: true }
);

// 页面加载时刷新配置与设备信息，避免其它页面更新配置后这里显示旧数据
onMounted(async () => {
  await load_config();
  await refreshLudusaviManifestStatus();
  fetchSystemFonts(); // Load in background, no await needed
});

watch(
  () => config.value.settings.locale,
  (new_locale) => {
    if (suppressConfigChangeTracking) {
      return;
    }
    info(`locale changed to ${new_locale}`);
    if (new_locale) {
      i18n.global.locale.value = new_locale as typeof i18n.global.locale.value;
    }
    notifyInfo($t('settings.locale_changed'));
  }
);

watch(
  () => config.value?.settings,
  async () => {
    if (suppressConfigChangeTracking) {
      return;
    }
    debouncedSaveConfig();
  },
  { deep: true } // 深度监听对象变化
);

const confirmBeforeApplyLatest = computed({
  get() {
    return config.value.settings.confirm_before_apply_latest !== false;
  },
  set(value: boolean) {
    config.value.settings.confirm_before_apply_latest = value;
  },
});

const confirmBeforeApplySnapshot = computed({
  get() {
    return config.value.settings.confirm_before_apply_snapshot !== false;
  },
  set(value: boolean) {
    config.value.settings.confirm_before_apply_snapshot = value;
  },
});

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
            <el-alert
              v-if="!hasBundledManifest"
              type="warning"
              :closable="false"
              class="manifest-hint"
            >
              {{ $t('settings.ludusavi_manifest_slim_hint') }}
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
                  {{
                    hasBundledManifest
                      ? $t('settings.manifest_reset')
                      : $t('settings.manifest_clear_local')
                  }}
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
          <div class="setting-hint">
            <span>{{ $t('settings.max_auto_backup_count_hint') }}</span>
          </div>
          <div class="setting-box">
            <ElSwitch v-model="config.settings.extra_backup_when_apply" />
            <span class="setting-label">{{ $t('settings.extra_backup_when_apply') }}</span>
          </div>
          <div class="setting-box">
            <ElInputNumber
              v-model="config.settings.max_extra_backup_count"
              :min="0"
              :max="999"
              :step="1"
              :disabled="!config.settings.extra_backup_when_apply"
            />
            <span class="setting-label">{{ $t('settings.max_extra_backup_count') }}</span>
          </div>
          <div class="setting-box">
            <ElSwitch v-model="config.settings.default_delete_before_apply" />
            <span class="setting-label">{{ $t('settings.default_delete_before_apply') }}</span>
          </div>
          <div class="setting-box">
            <ElSwitch v-model="confirmBeforeApplyLatest" />
            <span class="setting-label">{{ $t('settings.confirm_before_apply_latest') }}</span>
          </div>
          <div class="setting-box">
            <ElSwitch v-model="confirmBeforeApplySnapshot" />
            <span class="setting-label">{{ $t('settings.confirm_before_apply_snapshot') }}</span>
          </div>
          <div class="setting-box">
            <ElSelect v-model="config.settings.compression_preset" style="width: 160px">
              <ElOption label="Store" value="Store" />
              <ElOption label="Fast (Deflate)" value="Fast" />
              <ElOption label="Standard (Zstd)" value="Standard" />
              <ElOption label="Best (Zstd L19)" value="Best" />
            </ElSelect>
            <span class="setting-label">{{ $t('settings.compression_preset') }}</span>
          </div>
          <div class="setting-box">
            <ElSwitch v-model="config.settings.add_new_to_favorites" />
            <span class="setting-label">{{ $t('settings.add_new_to_favorites') }}</span>
          </div>
          <div class="setting-box">
            <ElSwitch v-model="config.settings.compute_archive_hash" />
            <span class="setting-label">{{ $t('settings.compute_archive_hash') }}</span>
          </div>
          <div class="setting-box">
            <ElSwitch
              v-model="config.settings.verify_archive_before_apply"
              :disabled="!config.settings.compute_archive_hash"
            />
            <span class="setting-label">{{ $t('settings.verify_archive_before_apply') }}</span>
          </div>

          <el-divider content-position="left">
            <el-icon>
              <Search />
            </el-icon>
            <span class="tab-title">{{ $t('settings.vn_scanner') }}</span>
          </el-divider>

          <div class="setting-box">
            <span class="setting-label">{{ $t('settings.vn_scan_dirs') }}</span>
            <el-button size="small" type="primary" @click="addVnScanDir">
              {{ $t('settings.add_scan_dir') }}
            </el-button>
          </div>
          <div class="setting-box">
            <template v-if="(config.settings.vn_scan_dirs ?? []).length > 0">
              <el-tag
                v-for="dir in config.settings.vn_scan_dirs ?? []"
                :key="dir"
                closable
                class="scan-dir-tag"
                @close="removeVnScanDir(dir)"
              >
                {{ dir }}
              </el-tag>
            </template>
            <span v-else class="setting-hint">{{ $t('settings.no_scan_dirs') }}</span>
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
            <ElSwitch v-model="isDark" />
            <span class="setting-label">{{ $t('settings.enable_dark_mode') }}</span>
          </div>
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

          <!-- 游戏根目录 -->
          <div class="setting-box">
            <h3>{{ $t('settings.game_roots_title') }}</h3>
            <p class="setting-hint">{{ $t('settings.game_roots_hint') }}</p>
            <div class="game-roots-list">
              <div v-for="(root, index) in gameRootResources" :key="root.id" class="game-root-item">
                <el-select
                  v-if="root.kind.type === 'gameRoot'"
                  v-model="root.kind.store"
                  style="width: 140px"
                  @change="saveGameRoots"
                >
                  <el-option label="Steam" value="steam" />
                  <el-option label="GOG" value="gog" />
                  <el-option label="Microsoft" value="microsoft" />
                  <el-option label="Ubisoft" value="uplay" />
                  <el-option :label="$t('settings.store_other')" value="other" />
                </el-select>
                <el-input
                  :model-value="root.kind.type === 'gameRoot' ? root.kind.path : ''"
                  :placeholder="$t('settings.game_roots_path_placeholder')"
                  @update:model-value="(val: string) => updateGameRoot(index, val)"
                  @change="saveGameRoots"
                />
                <el-button text @click="pickGameRoot(index)">
                  <el-icon><FolderOpened /></el-icon>
                </el-button>
                <el-button text type="danger" @click="removeGameRoot(index)">
                  <el-icon><Close /></el-icon>
                </el-button>
              </div>
            </div>
            <div class="game-roots-actions">
              <el-button size="small" @click="addGameRoot">
                {{ $t('settings.game_roots_add') }}
              </el-button>
              <el-button
                size="small"
                type="primary"
                :loading="detectingGameRoots"
                @click="autoDetectGameRoots"
              >
                <el-icon><Search /></el-icon>
                {{ $t('settings.game_roots_auto_detect') }}
              </el-button>
            </div>
          </div>

          <div class="setting-box">
            <h3>{{ $t('settings.store_accounts_title') }}</h3>
            <p class="setting-hint">{{ $t('settings.store_accounts_hint') }}</p>
            <div class="game-roots-list">
              <div
                v-for="account in storeAccountResources"
                :key="account.id"
                class="game-root-item"
              >
                <template v-if="account.kind.type === 'storeAccount'">
                  <el-select
                    v-model="account.kind.store"
                    style="width: 140px"
                    @change="persistDeviceInfo(false)"
                  >
                    <el-option label="Steam" value="steam" />
                    <el-option label="GOG" value="gog" />
                    <el-option label="Microsoft" value="microsoft" />
                    <el-option label="Ubisoft" value="uplay" />
                    <el-option :label="$t('settings.store_other')" value="other" />
                  </el-select>
                  <el-input
                    v-model="account.kind.user_id"
                    :placeholder="$t('settings.store_account_id_placeholder')"
                    @change="persistDeviceInfo(false)"
                  />
                  <el-button text type="danger" @click="removeStoreAccount(account.id)">
                    <el-icon><Close /></el-icon>
                  </el-button>
                </template>
              </div>
            </div>
            <el-button size="small" @click="addStoreAccount">
              {{ $t('settings.store_account_add') }}
            </el-button>
          </div>

          <div class="setting-box">
            <h3>{{ $t('settings.game_installations_title') }}</h3>
            <p class="setting-hint">{{ $t('settings.game_installations_hint') }}</p>
            <div class="game-roots-list">
              <div
                v-for="installation in installationResources"
                :key="installation.id"
                class="game-root-item installation-item"
              >
                <template v-if="installation.kind.type === 'gameInstallation'">
                  <el-select
                    v-model="installation.kind.root_id"
                    :placeholder="$t('settings.game_installation_root')"
                    style="width: 180px"
                    @change="persistDeviceInfo(false)"
                  >
                    <el-option
                      v-for="root in gameRootResources"
                      :key="root.id"
                      :label="root.kind.type === 'gameRoot' ? root.kind.path : String(root.id)"
                      :value="root.id"
                    />
                  </el-select>
                  <el-input
                    v-model="installation.kind.install_dir"
                    :placeholder="$t('settings.game_installation_name')"
                    @change="persistDeviceInfo(false)"
                  />
                  <el-input
                    v-model="installation.kind.path"
                    :placeholder="$t('settings.game_installation_path')"
                    @change="persistDeviceInfo(false)"
                  />
                  <el-button text type="danger" @click="removeGameInstallation(installation.id)">
                    <el-icon><Close /></el-icon>
                  </el-button>
                </template>
              </div>
            </div>
            <el-button size="small" @click="addGameInstallation">
              {{ $t('settings.game_installation_add') }}
            </el-button>
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
              <strong v-if="currentQuickActionGame">
                {{ $t('setting.current_quick_action_game') }} :
                {{ currentQuickActionGame.name }}
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
            <div class="quick-action-row">
              <ElSwitch v-model="config.quick_action!.notify_when_unchanged" />
              <span class="setting-label">{{
                $t('settings.quick_action_notify_when_unchanged')
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
            <span class="tab-title">{{ $t('settings.save_list_sort_settings') }}</span>
          </el-divider>

          <div class="setting-box">
            <div class="sort-settings-row">
              <ElSelect
                v-model="config.settings.save_list_sort_mode"
                style="width: 180px"
                @change="onSaveListSortModeChange"
              >
                <ElOption :label="$t('settings.save_list_sort_saved_order')" value="saved_order" />
                <ElOption :label="$t('settings.save_list_sort_last_played')" value="last_played" />
                <ElOption :label="$t('settings.save_list_sort_name')" value="name" />
              </ElSelect>
              <span class="setting-label">{{ $t('settings.save_list_sort_mode') }}</span>
            </div>
            <div class="sort-settings-row">
              <ElSelect v-model="config.settings.save_list_sort_direction" style="width: 180px">
                <ElOption :label="$t('settings.save_list_sort_ascending')" value="asc" />
                <ElOption :label="$t('settings.save_list_sort_descending')" value="desc" />
              </ElSelect>
              <span class="setting-label">{{ $t('settings.save_list_sort_direction') }}</span>
            </div>
          </div>

          <el-divider content-position="left">
            <el-icon>
              <Tools />
            </el-icon>
            <span class="tab-title">{{ $t('settings.edit_default_game_order') }}</span>
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

.setting-hint {
  margin-left: 10px;
  padding: 4px 10px;
  font-size: 12px;
  color: var(--el-text-color-secondary);
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

.game-roots-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 8px 0;
}

.game-root-item {
  display: flex;
  align-items: center;
  gap: 4px;
}

.game-roots-actions {
  display: flex;
  gap: 8px;
  margin-top: 8px;
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

.sort-settings-row {
  display: flex;
  align-items: center;
  margin-bottom: 10px;
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
