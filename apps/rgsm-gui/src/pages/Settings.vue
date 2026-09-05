<script lang="ts" setup>
// TODO:调整日志设置，比如删除日
import { computed, nextTick, ref, watch, onMounted } from 'vue';
import { $t, getSupportedLanguages, i18n } from '../i18n';
import draggable from 'vuedraggable';
import {
  Archive,
  BookText,
  Copy,
  Download,
  Eye,
  EyeOff,
  FolderSearch,
  FolderOpen,
  GripVertical,
  Keyboard,
  KeyRound,
  ListOrdered,
  LoaderCircle,
  MonitorSmartphone,
  Palette,
  Package,
  PanelsTopLeft,
  Play,
  Plus,
  RefreshCw,
  ScanSearch,
  Settings,
  Users,
  X,
  Zap,
} from '@lucide/vue';
import { KAlert, KButton, KInput, KNumberInput, KSelect, KSwitch, KTag } from '../ui/kit';
import CloudDeviceProfilesPanel from '../components/CloudDeviceProfilesPanel.vue';
import HotkeySelector from '../components/HotkeySelector.vue';
import { useNavigationLinks } from '../composables/useNavigationLinks';
import { useDark, useDebounceFn } from '@vueuse/core';
import { commands } from '~/api/commands';
import type {
  QuickActionSoundPreferences,
  QuickActionSoundSlots,
  QuickActionSoundSource,
  QuickActionsSettings,
  LudusaviManifestStatus,
} from '~/api/commands';
import { error, info } from '../utils/logger';
import type { CloudNamespaceGeneration, Device } from '../api/commands';
import { saveUnitPaths } from '../utils/saveUnit';
import { applyGameOrder } from '../utils/gameOrder';

const isDark = useDark();
const { config, refreshConfig, saveConfig } = useConfig();
const feedback = useFeedback();
const currentQuickActionGame = computed(() => {
  const identity = config.value.quick_action?.quick_action_game_id;
  if (!identity) return undefined;
  return config.value.games.find((game) => game.storage_key === identity || game.name === identity);
});
const httpHostBaseUrl = ref('');
const httpApiToken = ref('');
const rotatingHttpApiToken = ref(false);

async function loadHttpHostInfo() {
  try {
    const result = await commands.getHttpHostInfo();
    if (result.status === 'error') {
      notifyError($t('settings.local_api_load_failed'), result.error);
      return;
    }
    httpHostBaseUrl.value = result.data.baseUrl;
    httpApiToken.value = result.data.token;
  } catch (reason) {
    notifyError($t('settings.local_api_load_failed'), String(reason));
  }
}

async function copyHttpApiToken() {
  await navigator.clipboard.writeText(httpApiToken.value);
  notifySuccess($t('settings.local_api_token_copied'));
}

async function regenerateHttpApiToken() {
  try {
    await feedback.confirm(
      $t('settings.local_api_regenerate_warning'),
      $t('settings.local_api_regenerate_title'),
      {
        confirmButtonText: $t('settings.local_api_regenerate'),
        cancelButtonText: $t('settings.cancel'),
        type: 'warning',
      }
    );
  } catch {
    return;
  }

  rotatingHttpApiToken.value = true;
  try {
    const result = await commands.regenerateHttpApiToken();
    if (result.status === 'error') {
      notifyError($t('settings.local_api_regenerate_failed'), result.error);
      return;
    }
    httpApiToken.value = result.data.token;
    notifySuccess($t('settings.local_api_regenerated'));
  } finally {
    rotatingHttpApiToken.value = false;
  }
}

const cloudNamespaceGeneration = ref<CloudNamespaceGeneration | null>(null);
const v2LibraryActive = computed(() => cloudNamespaceGeneration.value === 'v2');
const hotkeysChanged = ref(false);
const draftGameOrder = ref<string[] | null>(null);
const gameOrderChanged = computed(() => draftGameOrder.value !== null);
const orderedGames = computed({
  get: () =>
    draftGameOrder.value
      ? applyGameOrder(config.value.games, draftGameOrder.value)
      : config.value.games,
  set: (games) => {
    draftGameOrder.value = games.map((game) => game.storage_key || game.name);
  },
});
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

async function confirmResetSettings() {
  try {
    await feedback.confirm($t('settings.confirm_reset'), $t('settings.reset_settings'), {
      confirmButtonText: $t('settings.reset_settings'),
      cancelButtonText: $t('settings.cancel'),
      type: 'warning',
    });
  } catch {
    return;
  }
  await reset_settings();
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
    config.value.games = orderedGames.value;
    if (!(await saveConfig())) return;
    draftGameOrder.value = null;
    // 只显示功能完成的消息，而不是保存成功
    notifySuccess($t('settings.game_order_saved'));
  } catch (e) {
    error(`save game order error: ${e}`);
    notifyError($t('error.set_config_failed'));
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

// 页面加载时刷新配置与设备信息，避免其它页面更新配置后这里显示旧数据
onMounted(async () => {
  void loadHttpHostInfo();
  await load_config();
  await refreshLudusaviManifestStatus();
  fetchSystemFonts(); // Load in background, no await needed
  const generation = await commands.getCloudNamespaceGeneration();
  if (generation.status === 'ok') {
    cloudNamespaceGeneration.value = generation.data;
  }
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

const localeOptions = getSupportedLanguages().map((lang) => ({
  value: lang.code,
  label: `${lang.name} - ${lang.code}`,
}));
const homePageOptions = computed(() =>
  router_list.value.map((route) => ({ value: route.link, label: route.text }))
);
const compressionOptions = [
  { label: 'Store', value: 'Store' },
  { label: 'Fast (Deflate)', value: 'Fast' },
  { label: 'Standard (Zstd)', value: 'Standard' },
  { label: 'Best (Zstd L19)', value: 'Best' },
];
const expandBehaviorOptions = computed(() => [
  { label: $t('settings.save_list_expand_behavior_default_open'), value: 'always_open' },
  { label: $t('settings.save_list_expand_behavior_default_closed'), value: 'always_closed' },
  { label: $t('settings.save_list_expand_behavior_remember_last'), value: 'remember_last' },
]);
const sortModeOptions = computed(() => [
  { label: $t('settings.save_list_sort_saved_order'), value: 'saved_order' },
  { label: $t('settings.save_list_sort_last_played'), value: 'last_played' },
  { label: $t('settings.save_list_sort_name'), value: 'name' },
]);
const sortDirectionOptions = computed(() => [
  { label: $t('settings.save_list_sort_ascending'), value: 'asc' },
  { label: $t('settings.save_list_sort_descending'), value: 'desc' },
]);
const storeOptions = computed(() => [
  { label: 'Steam', value: 'steam' },
  { label: 'GOG', value: 'gog' },
  { label: 'Microsoft', value: 'microsoft' },
  { label: 'Ubisoft', value: 'uplay' },
  { label: $t('settings.store_other'), value: 'other' },
]);
const soundModeOptions = computed(() => [
  { label: $t('settings.quick_action_sound_mode_default'), value: 'default' },
  { label: $t('settings.quick_action_sound_mode_custom'), value: 'file' },
]);
const rootResourceOptions = computed(() =>
  gameRootResources.value.map((root) => ({
    value: root.id,
    label: root.kind.type === 'gameRoot' ? root.kind.path : String(root.id),
  }))
);

/** Number settings: 0 is meaningful, empty field maps to 0 via bridge. */
const maxAutoBackupCount = computed({
  get: () => config.value.settings.max_auto_backup_count,
  set: (value: number | undefined) => {
    config.value.settings.max_auto_backup_count = value ?? 0;
  },
});
const maxExtraBackupCount = computed({
  get: () => config.value.settings.max_extra_backup_count,
  set: (value: number | undefined) => {
    config.value.settings.max_extra_backup_count = value ?? 0;
  },
});

const showHttpToken = ref(false);
const fontListId = `font-options-${Math.random().toString(36).slice(2, 8)}`;

type SettingsSection = 'general' | 'scan' | 'backup' | 'ui' | 'device' | 'hotkeys' | 'order';
const activeSection = ref<SettingsSection>('general');
const sectionNav = computed(() => [
  { key: 'general' as const, icon: Settings, label: $t('settings.general') },
  { key: 'scan' as const, icon: ScanSearch, label: $t('settings.section_auto_scan') },
  { key: 'backup' as const, icon: Archive, label: $t('settings.backup_settings') },
  { key: 'ui' as const, icon: Palette, label: $t('settings.ui_appearance') },
  { key: 'device' as const, icon: MonitorSmartphone, label: $t('settings.device_settings') },
  { key: 'hotkeys' as const, icon: Zap, label: $t('settings.section_quick_action') },
  { key: 'order' as const, icon: ListOrdered, label: $t('settings.game_order') },
]);

const { linksWithGames: router_list } = useNavigationLinks();
</script>

<template>
  <div class="h-full overflow-y-auto">
    <div class="mx-auto flex max-w-[960px] gap-10 px-6 py-6">
      <aside class="sticky top-6 w-44 shrink-0 self-start">
        <h1 class="mb-4 px-2 text-lg font-semibold text-text">
          {{ $t('settings.customizable_settings') }}
        </h1>
        <nav class="flex flex-col gap-0.5" :aria-label="$t('settings.customizable_settings')">
          <button
            v-for="item in sectionNav"
            :key="item.key"
            type="button"
            class="flex cursor-pointer items-center gap-2 rounded-sm border-none bg-transparent px-2 py-1.5 text-left text-[13px] transition-colors focus-visible:outline-2 focus-visible:outline-accent"
            :class="
              activeSection === item.key
                ? 'bg-surface-2 font-semibold text-text'
                : 'text-text-dim hover:bg-surface-2/60 hover:text-text'
            "
            :aria-current="activeSection === item.key ? 'page' : undefined"
            @click="activeSection = item.key"
          >
            <component :is="item.icon" :size="14" aria-hidden="true" />
            {{ item.label }}
          </button>
        </nav>
      </aside>
      <div class="min-w-0 max-w-[640px] flex-1 pb-16">
        <div v-if="activeSection === 'general'" class="flex flex-col gap-8">
          <!-- 通用 -->
          <section>
            <div class="mb-3 flex items-center gap-2 border-b border-border pb-2">
              <Settings :size="15" class="text-text-dim" aria-hidden="true" />
              <h2 class="text-sm font-semibold text-text">{{ $t('settings.general') }}</h2>
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{ $t('home.choose_language') }}</span>
              <div class="flex items-center gap-2">
                <KSelect
                  v-model="config.settings.locale"
                  class="w-56"
                  :options="localeOptions"
                  :aria-label="$t('home.choose_language')"
                />
              </div>
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{ $t('settings.homepage') }}</span>
              <KSelect
                v-model="config.settings.home_page"
                class="w-56"
                :options="homePageOptions"
                :aria-label="$t('settings.homepage')"
              />
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{ $t('settings.exit_to_tray') }}</span>
              <KSwitch v-model="config.settings.exit_to_tray" />
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{ $t('settings.log_to_file') }}</span>
              <KSwitch v-model="config.settings.log_to_file" />
            </div>
            <div class="mt-3 flex items-center gap-2 border-t border-border pt-4">
              <KButton size="sm" @click="open_log_folder()">
                <template #icon><FolderOpen :size="13" aria-hidden="true" /></template>
                {{ $t('settings.open_log_folder') }}
              </KButton>
              <KButton size="sm" variant="danger" @click="confirmResetSettings">
                {{ $t('settings.reset_settings') }}
              </KButton>
            </div>
          </section>

          <!-- 本机 API -->
          <section>
            <div class="mb-3 flex items-center gap-2 border-b border-border pb-2">
              <KeyRound :size="15" class="text-text-dim" aria-hidden="true" />
              <h2 class="text-sm font-semibold text-text">{{ $t('settings.local_api') }}</h2>
            </div>
            <KAlert tone="info" class="mb-3">{{ $t('settings.local_api_hint') }}</KAlert>
            <div class="flex items-start justify-between gap-4 py-1">
              <span class="shrink-0 text-xs text-text-dim">{{
                $t('settings.local_api_endpoint')
              }}</span>
              <span class="break-all text-right font-mono text-xs text-text">{{
                httpHostBaseUrl
              }}</span>
            </div>
            <div class="flex items-center justify-between gap-4 py-1">
              <span class="shrink-0 text-xs text-text-dim">{{
                $t('settings.local_api_token')
              }}</span>
              <div class="flex min-w-0 items-center gap-1">
                <KInput
                  :model-value="httpApiToken"
                  class="w-64"
                  :type="showHttpToken ? 'text' : 'password'"
                  readonly
                  mono
                  :aria-label="$t('settings.local_api_token')"
                />
                <KButton
                  variant="ghost"
                  size="sm"
                  :aria-label="showHttpToken ? $t('common.hide') : $t('common.show')"
                  @click="showHttpToken = !showHttpToken"
                >
                  <template #icon>
                    <EyeOff v-if="showHttpToken" :size="14" aria-hidden="true" />
                    <Eye v-else :size="14" aria-hidden="true" />
                  </template>
                </KButton>
                <KButton
                  variant="ghost"
                  size="sm"
                  :aria-label="$t('settings.local_api_copy_token')"
                  @click="copyHttpApiToken"
                >
                  <template #icon><Copy :size="14" aria-hidden="true" /></template>
                </KButton>
              </div>
            </div>
            <div class="mt-3 flex justify-end">
              <KButton
                size="sm"
                variant="danger"
                :loading="rotatingHttpApiToken"
                @click="regenerateHttpApiToken"
              >
                {{ $t('settings.local_api_regenerate') }}
              </KButton>
            </div>
          </section>
        </div>
        <div v-else-if="activeSection === 'scan'" class="flex flex-col gap-8">
          <!-- Ludusavi 清单 -->
          <section>
            <div class="mb-3 flex items-center gap-2 border-b border-border pb-2">
              <BookText :size="15" class="text-text-dim" aria-hidden="true" />
              <h2 class="text-sm font-semibold text-text">
                {{ $t('settings.ludusavi_manifest') }}
              </h2>
            </div>
            <KAlert tone="info" class="mb-2">{{ $t('settings.ludusavi_manifest_hint') }}</KAlert>
            <KAlert v-if="!hasBundledManifest" tone="warning" class="mb-3">
              {{ $t('settings.ludusavi_manifest_slim_hint') }}
            </KAlert>
            <div class="relative rounded-md bg-surface-2 p-3">
              <div
                v-if="ludusaviManifestLoading"
                class="absolute inset-0 z-10 flex items-center justify-center rounded-md bg-surface-2/70"
              >
                <LoaderCircle :size="20" class="animate-spin text-text-dim" aria-hidden="true" />
              </div>
              <div class="flex items-start justify-between gap-4 py-1">
                <span class="text-xs text-text-dim">{{ $t('settings.manifest_source') }}</span>
                <span class="font-mono text-xs text-text">{{
                  formatManifestSource(ludusaviManifest?.source)
                }}</span>
              </div>
              <div class="flex items-start justify-between gap-4 py-1">
                <span class="text-xs text-text-dim">{{ $t('settings.manifest_updated_at') }}</span>
                <span class="font-mono text-xs text-text">{{
                  ludusaviManifest?.updatedAt || '-'
                }}</span>
              </div>
              <div class="flex items-start justify-between gap-4 py-1">
                <span class="text-xs text-text-dim">{{ $t('settings.manifest_etag') }}</span>
                <span class="font-mono text-xs text-text">{{
                  formatManifestEtag(ludusaviManifest?.etag)
                }}</span>
              </div>
            </div>
            <div class="mt-3 flex items-center justify-between">
              <KButton size="sm" @click="refreshLudusaviManifestStatus">
                <template #icon><RefreshCw :size="13" aria-hidden="true" /></template>
                {{ $t('settings.manifest_refresh') }}
              </KButton>
              <div class="flex gap-2">
                <KButton
                  size="sm"
                  variant="primary"
                  :loading="ludusaviManifestUpdating"
                  @click="updateLudusaviManifest"
                >
                  {{ $t('settings.manifest_update') }}
                </KButton>
                <KButton
                  size="sm"
                  variant="danger"
                  :loading="ludusaviManifestResetting"
                  @click="resetLudusaviManifest"
                >
                  {{
                    hasBundledManifest
                      ? $t('settings.manifest_reset')
                      : $t('settings.manifest_clear_local')
                  }}
                </KButton>
              </div>
            </div>
          </section>

          <!-- 游戏根目录 -->
          <section>
            <div class="mb-3 flex items-center gap-2 border-b border-border pb-2">
              <FolderSearch :size="15" class="text-text-dim" aria-hidden="true" />
              <h2 class="text-sm font-semibold text-text">{{ $t('settings.game_roots_title') }}</h2>
            </div>
            <p class="mb-2 text-xs leading-relaxed text-text-dim">
              {{ $t('settings.game_roots_hint') }}
            </p>
            <div class="flex flex-col gap-2">
              <div
                v-for="(root, index) in gameRootResources"
                :key="root.id"
                class="flex items-center gap-2"
              >
                <KSelect
                  v-if="root.kind.type === 'gameRoot'"
                  v-model="root.kind.store"
                  class="w-32 shrink-0"
                  :options="storeOptions"
                  :aria-label="$t('settings.game_roots_title')"
                  @update:model-value="saveGameRoots"
                />
                <KInput
                  class="w-full"
                  :model-value="root.kind.type === 'gameRoot' ? root.kind.path : ''"
                  mono
                  :placeholder="$t('settings.game_roots_path_placeholder')"
                  :aria-label="$t('settings.game_roots_path_placeholder')"
                  @update:model-value="updateGameRoot(index, String($event ?? ''))"
                  @change="saveGameRoots"
                />
                <KButton
                  variant="ghost"
                  size="sm"
                  :aria-label="$t('save_location_drawer.pick_path')"
                  @click="pickGameRoot(index)"
                >
                  <template #icon><FolderOpen :size="14" aria-hidden="true" /></template>
                </KButton>
                <KButton
                  variant="ghost"
                  size="sm"
                  class="text-danger"
                  :aria-label="$t('addgame.remove')"
                  @click="removeGameRoot(index)"
                >
                  <template #icon><X :size="14" aria-hidden="true" /></template>
                </KButton>
              </div>
            </div>
            <div class="mt-2 flex gap-2">
              <KButton size="sm" @click="addGameRoot">
                <template #icon><Plus :size="13" aria-hidden="true" /></template>
                {{ $t('settings.game_roots_add') }}
              </KButton>
              <KButton
                size="sm"
                variant="primary"
                :loading="detectingGameRoots"
                @click="autoDetectGameRoots"
              >
                <template #icon><ScanSearch :size="13" aria-hidden="true" /></template>
                {{ $t('settings.game_roots_auto_detect') }}
              </KButton>
            </div>
          </section>

          <!-- 商店账号 -->
          <section>
            <div class="mb-3 flex items-center gap-2 border-b border-border pb-2">
              <Users :size="15" class="text-text-dim" aria-hidden="true" />
              <h2 class="text-sm font-semibold text-text">
                {{ $t('settings.store_accounts_title') }}
              </h2>
            </div>
            <p class="mb-2 text-xs leading-relaxed text-text-dim">
              {{ $t('settings.store_accounts_hint') }}
            </p>
            <div class="flex flex-col gap-2">
              <div
                v-for="account in storeAccountResources"
                :key="account.id"
                class="flex items-center gap-2"
              >
                <template v-if="account.kind.type === 'storeAccount'">
                  <KSelect
                    v-model="account.kind.store"
                    class="w-32 shrink-0"
                    :options="storeOptions"
                    :aria-label="$t('settings.game_roots_title')"
                    @update:model-value="persistDeviceInfo(false)"
                  />
                  <KInput
                    v-model="account.kind.user_id"
                    class="w-full"
                    mono
                    :placeholder="$t('settings.store_account_id_placeholder')"
                    :aria-label="$t('settings.store_account_id_placeholder')"
                    @change="persistDeviceInfo(false)"
                  />
                  <KButton
                    variant="ghost"
                    size="sm"
                    class="text-danger"
                    :aria-label="$t('addgame.remove')"
                    @click="removeStoreAccount(account.id)"
                  >
                    <template #icon><X :size="14" aria-hidden="true" /></template>
                  </KButton>
                </template>
              </div>
            </div>
            <div class="mt-2">
              <KButton size="sm" @click="addStoreAccount">
                <template #icon><Plus :size="13" aria-hidden="true" /></template>
                {{ $t('settings.store_account_add') }}
              </KButton>
            </div>
          </section>

          <!-- 游戏安装位置 -->
          <section>
            <div class="mb-3 flex items-center gap-2 border-b border-border pb-2">
              <Package :size="15" class="text-text-dim" aria-hidden="true" />
              <h2 class="text-sm font-semibold text-text">
                {{ $t('settings.game_installations_title') }}
              </h2>
            </div>
            <p class="mb-2 text-xs leading-relaxed text-text-dim">
              {{ $t('settings.game_installations_hint') }}
            </p>
            <div class="flex flex-col gap-2">
              <div
                v-for="installation in installationResources"
                :key="installation.id"
                class="flex flex-wrap items-center gap-2"
              >
                <template v-if="installation.kind.type === 'gameInstallation'">
                  <KSelect
                    v-model="installation.kind.root_id"
                    class="w-44 shrink-0"
                    :options="rootResourceOptions"
                    :placeholder="$t('settings.game_installation_root')"
                    :aria-label="$t('settings.game_installation_root')"
                    @update:model-value="persistDeviceInfo(false)"
                  />
                  <KInput
                    v-model="installation.kind.install_dir"
                    class="min-w-32 flex-1"
                    :placeholder="$t('settings.game_installation_name')"
                    :aria-label="$t('settings.game_installation_name')"
                    @change="persistDeviceInfo(false)"
                  />
                  <KInput
                    v-model="installation.kind.path"
                    mono
                    class="min-w-40 flex-1"
                    :placeholder="$t('settings.game_installation_path')"
                    :aria-label="$t('settings.game_installation_path')"
                    @change="persistDeviceInfo(false)"
                  />
                  <KButton
                    variant="ghost"
                    size="sm"
                    class="text-danger"
                    :aria-label="$t('addgame.remove')"
                    @click="removeGameInstallation(installation.id)"
                  >
                    <template #icon><X :size="14" aria-hidden="true" /></template>
                  </KButton>
                </template>
              </div>
            </div>
            <div class="mt-2">
              <KButton size="sm" @click="addGameInstallation">
                <template #icon><Plus :size="13" aria-hidden="true" /></template>
                {{ $t('settings.game_installation_add') }}
              </KButton>
            </div>
          </section>

          <!-- VN 扫描 -->
          <section>
            <div class="mb-3 flex items-center gap-2 border-b border-border pb-2">
              <ScanSearch :size="15" class="text-text-dim" aria-hidden="true" />
              <h2 class="text-sm font-semibold text-text">{{ $t('settings.vn_scanner') }}</h2>
            </div>
            <p class="mb-3 text-xs leading-relaxed text-text-dim">
              {{ $t('settings.vn_scanner_hint') }}
            </p>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{ $t('settings.vn_scan_dirs') }}</span>
              <KButton size="sm" variant="primary" @click="addVnScanDir">
                <template #icon><Plus :size="13" aria-hidden="true" /></template>
                {{ $t('settings.add_scan_dir') }}
              </KButton>
            </div>
            <div
              v-if="(config.settings.vn_scan_dirs ?? []).length > 0"
              class="flex flex-col gap-1.5"
            >
              <div
                v-for="dir in config.settings.vn_scan_dirs ?? []"
                :key="dir"
                class="flex items-center justify-between gap-2 rounded-sm border border-border px-2.5 py-1.5"
              >
                <span class="truncate font-mono text-xs text-text">{{ dir }}</span>
                <KButton
                  variant="ghost"
                  size="sm"
                  :aria-label="$t('addgame.remove')"
                  @click="removeVnScanDir(dir)"
                >
                  <template #icon><X :size="13" aria-hidden="true" /></template>
                </KButton>
              </div>
            </div>
            <p v-else class="py-1 text-xs text-text-dim">{{ $t('settings.no_scan_dirs') }}</p>
          </section>
        </div>
        <div v-else-if="activeSection === 'backup'" class="flex flex-col gap-8">
          <!-- 备份设置 -->
          <section>
            <div class="mb-3 flex items-center gap-2 border-b border-border pb-2">
              <Archive :size="15" class="text-text-dim" aria-hidden="true" />
              <h2 class="text-sm font-semibold text-text">{{ $t('settings.backup_settings') }}</h2>
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{
                $t('settings.prompt_when_auto_backup')
              }}</span>
              <KSwitch v-model="config.settings.prompt_when_auto_backup" />
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <div class="min-w-0 shrink-0">
                <span class="text-sm text-text">{{ $t('settings.max_auto_backup_count') }}</span>
                <p class="mt-0.5 text-xs leading-relaxed text-text-dim">
                  {{ $t('settings.max_auto_backup_count_hint') }}
                </p>
              </div>
              <KNumberInput v-model="maxAutoBackupCount" :min="0" :max="999" class="w-28" />
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{
                $t('settings.extra_backup_when_apply')
              }}</span>
              <KSwitch v-model="config.settings.extra_backup_when_apply" />
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{
                $t('settings.max_extra_backup_count')
              }}</span>
              <KNumberInput
                v-model="maxExtraBackupCount"
                :min="0"
                :max="999"
                class="w-28"
                :disabled="!config.settings.extra_backup_when_apply"
              />
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{
                $t('settings.default_delete_before_apply')
              }}</span>
              <KSwitch v-model="config.settings.default_delete_before_apply" />
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{
                $t('settings.confirm_before_apply_latest')
              }}</span>
              <KSwitch v-model="confirmBeforeApplyLatest" />
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{
                $t('settings.confirm_before_apply_snapshot')
              }}</span>
              <KSwitch v-model="confirmBeforeApplySnapshot" />
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{
                $t('settings.compression_preset')
              }}</span>
              <KSelect
                v-model="config.settings.compression_preset"
                class="w-44"
                :options="compressionOptions"
                :aria-label="$t('settings.compression_preset')"
              />
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{
                $t('settings.add_new_to_favorites')
              }}</span>
              <KSwitch v-model="config.settings.add_new_to_favorites" />
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{
                $t('settings.compute_archive_hash')
              }}</span>
              <KSwitch v-model="config.settings.compute_archive_hash" />
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{
                $t('settings.verify_archive_before_apply')
              }}</span>
              <KSwitch
                v-model="config.settings.verify_archive_before_apply"
                :disabled="!config.settings.compute_archive_hash"
              />
            </div>
            <div class="mt-3 flex items-center gap-2 border-t border-border pt-4">
              <KButton size="sm" @click="backup_all">
                {{ $t('settings.backup_all') }}
              </KButton>
              <KButton size="sm" variant="danger" @click="apply_all">
                {{ $t('settings.apply_all') }}
              </KButton>
            </div>
          </section>
        </div>
        <div v-else-if="activeSection === 'ui'" class="flex flex-col gap-8">
          <!-- 界面 -->
          <section>
            <div class="mb-3 flex items-center gap-2 border-b border-border pb-2">
              <PanelsTopLeft :size="15" class="text-text-dim" aria-hidden="true" />
              <h2 class="text-sm font-semibold text-text">{{ $t('settings.ui_settings') }}</h2>
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{
                $t('settings.save_list_expand_behavior')
              }}</span>
              <KSelect
                v-model="config.settings.save_list_expand_behavior"
                class="w-56"
                :options="expandBehaviorOptions"
                :aria-label="$t('settings.save_list_expand_behavior')"
              />
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{
                $t('settings.default_expend_favorites_tree')
              }}</span>
              <KSwitch v-model="config.settings.default_expend_favorites_tree" />
            </div>
          </section>

          <!-- 外观 -->
          <section>
            <div class="mb-3 flex items-center gap-2 border-b border-border pb-2">
              <Palette :size="15" class="text-text-dim" aria-hidden="true" />
              <h2 class="text-sm font-semibold text-text">
                {{ $t('settings.appearance_settings') }}
              </h2>
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{ $t('settings.enable_dark_mode') }}</span>
              <KSwitch v-model="isDark" />
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{
                $t('settings.custom_font_enabled')
              }}</span>
              <KSwitch v-model="config.settings.appearance!.custom_font_enabled" />
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{ $t('settings.ui_font_family') }}</span>
              <KInput
                v-model="config.settings.appearance!.ui_font_family"
                class="w-72"
                :list="fontListId"
                :disabled="!config.settings.appearance?.custom_font_enabled"
                :placeholder="$t('settings.ui_font_family_placeholder')"
                :aria-label="$t('settings.ui_font_family')"
              />
              <datalist :id="fontListId">
                <option v-for="font in fontOptions" :key="font" :value="font" />
              </datalist>
            </div>
            <KAlert tone="info" class="mt-2">{{ $t('settings.custom_font_hint') }}</KAlert>
          </section>
        </div>

        <div v-else-if="activeSection === 'order'" class="flex flex-col gap-8">
          <!-- 游戏排序 -->
          <section>
            <div class="mb-3 flex items-center gap-2 border-b border-border pb-2">
              <ListOrdered :size="15" class="text-text-dim" aria-hidden="true" />
              <h2 class="text-sm font-semibold text-text">
                {{ $t('settings.save_list_sort_settings') }}
              </h2>
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{
                $t('settings.save_list_sort_mode')
              }}</span>
              <KSelect
                v-model="config.settings.save_list_sort_mode"
                class="w-44"
                :options="sortModeOptions"
                :aria-label="$t('settings.save_list_sort_mode')"
                @update:model-value="onSaveListSortModeChange(String($event))"
              />
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{
                $t('settings.save_list_sort_direction')
              }}</span>
              <KSelect
                v-model="config.settings.save_list_sort_direction"
                class="w-44"
                :options="sortDirectionOptions"
                :aria-label="$t('settings.save_list_sort_direction')"
              />
            </div>

            <h3 class="mb-2 mt-4 text-xs font-medium text-text">
              {{ $t('settings.edit_default_game_order') }}
            </h3>
            <draggable
              v-model="orderedGames"
              item-key="storage_key"
              class="max-w-md"
              :force-fallback="true"
            >
              <template #item="{ element }">
                <div
                  :data-game-order-id="element.storage_key"
                  class="mb-1.5 flex cursor-move select-none items-center gap-2 rounded-sm border border-border bg-surface px-3 py-2 text-sm text-text transition-colors hover:bg-surface-2"
                >
                  <GripVertical :size="14" class="shrink-0 text-text-dim" aria-hidden="true" />
                  <span class="truncate">{{ element.name }}</span>
                </div>
              </template>
            </draggable>
            <div class="mt-3 flex items-center gap-2">
              <KButton
                variant="primary"
                size="sm"
                :disabled="!gameOrderChanged"
                @click="saveGameOrder"
              >
                {{ $t('settings.save_game_order') }}
              </KButton>
              <KTag v-if="gameOrderChanged" tone="warning">{{
                $t('settings.unsaved_changes')
              }}</KTag>
            </div>
          </section>
        </div>
        <div v-else-if="activeSection === 'device'" class="flex flex-col gap-8">
          <!-- 设备 -->
          <section>
            <div class="mb-3 flex items-center gap-2 border-b border-border pb-2">
              <MonitorSmartphone :size="15" class="text-text-dim" aria-hidden="true" />
              <h2 class="text-sm font-semibold text-text">{{ $t('settings.device_settings') }}</h2>
            </div>

            <h3 class="mb-2 text-xs font-medium text-text">{{ $t('settings.current_device') }}</h3>
            <div class="mb-1">
              <div class="mb-1 block text-xs text-text-dim">{{ $t('settings.device_name') }}</div>
              <KInput
                v-model="currentDevice.name"
                class="w-full"
                :aria-label="$t('settings.device_name')"
                @change="updateDeviceInfo"
              />
            </div>
            <div class="mb-4">
              <div class="mb-1 block text-xs text-text-dim">{{ $t('settings.device_id') }}</div>
              <KInput
                v-model="currentDevice.id"
                class="w-full"
                mono
                disabled
                :aria-label="$t('settings.device_id')"
              />
            </div>

            <CloudDeviceProfilesPanel v-if="v2LibraryActive" class="mt-5" />

            <h3 class="mb-2 mt-5 text-xs font-medium text-text">
              {{ $t('settings.other_devices') }}
            </h3>
            <div class="flex flex-col">
              <div
                v-for="device in otherDevices"
                :key="device.id"
                class="flex items-center justify-between gap-3 border-t border-border py-2"
              >
                <div class="min-w-0">
                  <div class="truncate text-sm text-text">{{ device.name }}</div>
                  <div class="truncate font-mono text-[11px] text-text-dim">{{ device.id }}</div>
                </div>
                <div class="flex shrink-0 gap-1.5">
                  <KButton size="sm" @click="importFromDevice(device.id)">
                    <template #icon><Download :size="13" aria-hidden="true" /></template>
                    {{ $t('settings.import_paths') }}
                  </KButton>
                  <KButton size="sm" variant="danger" @click="deleteDevice(device.id)">
                    {{ $t('settings.delete_device') }}
                  </KButton>
                </div>
              </div>
              <p v-if="otherDevices.length === 0" class="py-2 text-xs text-text-dim">-</p>
            </div>
          </section>
        </div>
        <div v-else-if="activeSection === 'hotkeys'" class="flex flex-col gap-8">
          <!-- 快捷键 -->
          <section>
            <div class="mb-3 flex items-center gap-2 border-b border-border pb-2">
              <Keyboard :size="15" class="text-text-dim" aria-hidden="true" />
              <h2 class="text-sm font-semibold text-text">{{ $t('settings.hotkey_settings') }}</h2>
            </div>
            <p v-if="currentQuickActionGame" class="mb-2 text-xs text-text-dim">
              {{ $t('setting.current_quick_action_game') }} :
              <span class="font-medium text-text">{{ currentQuickActionGame.name }}</span>
            </p>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{
                $t('settings.quick_action_enable_sound')
              }}</span>
              <KSwitch v-model="config.quick_action!.enable_sound" />
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{
                $t('settings.quick_action_enable_notification')
              }}</span>
              <KSwitch v-model="config.quick_action!.enable_notification" />
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="shrink-0 text-sm text-text">{{
                $t('settings.quick_action_notify_when_unchanged')
              }}</span>
              <KSwitch v-model="config.quick_action!.notify_when_unchanged" />
            </div>

            <div class="mt-3 rounded-md bg-surface-2 p-3">
              <h3 class="mb-2 text-xs font-medium text-text">
                {{ $t('settings.quick_action_sound_title') }}
              </h3>
              <div class="flex flex-wrap items-center gap-2 py-1">
                <span class="w-28 shrink-0 text-xs text-text">{{
                  $t('settings.quick_action_sound_success')
                }}</span>
                <KSelect
                  v-model="successSoundMode"
                  class="w-36"
                  :options="soundModeOptions"
                  :aria-label="$t('settings.quick_action_sound_success')"
                />
                <template v-if="successSoundMode === 'file'">
                  <KInput
                    v-model="successSoundPath"
                    mono
                    class="min-w-40 flex-1"
                    :placeholder="$t('settings.quick_action_sound_file_placeholder')"
                    :aria-label="$t('settings.quick_action_sound_file_placeholder')"
                  />
                  <KButton size="sm" @click="chooseSoundFile('success')">
                    {{ $t('settings.quick_action_sound_choose') }}
                  </KButton>
                </template>
                <KButton size="sm" variant="ghost" @click="togglePreview('success')">
                  <template #icon><Play :size="13" aria-hidden="true" /></template>
                  {{ $t('settings.quick_action_sound_preview_button') }}
                </KButton>
              </div>
              <div class="flex flex-wrap items-center gap-2 py-1">
                <span class="w-28 shrink-0 text-xs text-text">{{
                  $t('settings.quick_action_sound_failure')
                }}</span>
                <KSelect
                  v-model="failureSoundMode"
                  class="w-36"
                  :options="soundModeOptions"
                  :aria-label="$t('settings.quick_action_sound_failure')"
                />
                <template v-if="failureSoundMode === 'file'">
                  <KInput
                    v-model="failureSoundPath"
                    mono
                    class="min-w-40 flex-1"
                    :placeholder="$t('settings.quick_action_sound_file_placeholder')"
                    :aria-label="$t('settings.quick_action_sound_file_placeholder')"
                  />
                  <KButton size="sm" @click="chooseSoundFile('failure')">
                    {{ $t('settings.quick_action_sound_choose') }}
                  </KButton>
                </template>
                <KButton size="sm" variant="ghost" @click="togglePreview('failure')">
                  <template #icon><Play :size="13" aria-hidden="true" /></template>
                  {{ $t('settings.quick_action_sound_preview_button') }}
                </KButton>
              </div>
            </div>

            <div class="mt-4">
              <HotkeySelector v-model="config.quick_action!.hotkeys" />
            </div>
            <div class="mt-3 flex items-center gap-2">
              <KButton variant="primary" size="sm" :disabled="!hotkeysChanged" @click="saveHotkeys">
                {{ $t('settings.save_hotkeys') }}
              </KButton>
              <KTag v-if="hotkeysChanged" tone="warning">{{ $t('settings.unsaved_changes') }}</KTag>
            </div>
          </section>
        </div>
      </div>
    </div>
  </div>
</template>
