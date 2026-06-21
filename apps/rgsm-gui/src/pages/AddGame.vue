<script lang="ts" setup>
import {
  DocumentAdd,
  Check,
  RefreshRight,
  Download,
  InfoFilled,
  Search,
} from '@element-plus/icons-vue';
import { computed, reactive, ref, watchEffect } from 'vue';
import {
  commands,
  type GameDraft,
  type SaveUnitDraft,
  type Device,
  type ImportableGame,
  type SavePath,
  type PathCheckResult,
} from '../bindings';
import { $t } from '../i18n';
import { v4 as uuidv4 } from 'uuid';
import { error } from '@tauri-apps/plugin-log';
import PathVariableInput from '../components/PathVariableInput.vue';
import GameImportDialog from '../components/GameImportDialog.vue';
import GameImportCustomizeDialog from '../components/GameImportCustomizeDialog.vue';
import GameBatchImportDialog from '../components/GameBatchImportDialog.vue';

const route = useRoute();
const router = useRouter();

const feedback = useFeedback();
const { config, refreshConfig, saveConfig } = useConfig();
const buttons = [
  {
    text: $t('addgame.search_local'),
    type: 'primary',
    icon: Download,
    method: search_local,
  },
  {
    text: $t('addgame.scan_vns'),
    type: 'primary',
    icon: Search,
    method: scan_vns,
  },
  {
    text: $t('addgame.save_current_profile'),
    type: 'success',
    icon: Check,
    method: save,
  },
  {
    text: $t('addgame.reset_current_profile'),
    type: 'danger',
    icon: RefreshRight,
    method: reset_info,
  },
] as const;

const game_name = ref(''); // 写入游戏名
const save_paths = reactive<SaveUnitDraft[]>([]); // 选择游戏存档目录
const game_path = ref(''); // 选择游戏启动程序
const game_icon_src = ref('/orange.png');
const is_editing = ref(false); // 是否正在编辑已有的游戏
const editing_storage_key = ref(''); // storage_key of the game being edited
const currentDevice = ref<Device | null>(null); // 当前设备信息

// Import dialog state
const showImportDialog = ref(false);
const importDialogLoading = ref(false);
const importableGames = ref<ImportableGame[]>([]);

// Customize dialog state (single game)
const showCustomizeDialog = ref(false);
const customizeDialogLoading = ref(false);
const customizingGame = ref<ImportableGame | null>(null);
const customizingSavePaths = ref<SavePath[]>([]);

// Batch import dialog state (multiple games)
const showBatchImportDialog = ref(false);
const batchImportLoading = ref(false);
const batchImportGames = ref<ImportableGame[]>([]);
const batchGamePaths = ref<Record<string, SavePath[]>>({});

// Ludusavi metadata pending for next save() call (set during single-game import)
const pendingLudusaviMeta = ref<{ installDirs: string[]; steamId: number | null } | null>(null);
// Store user ID pending for next save() call (set during import)
const pendingStoreUserId = ref<string | null>(null);
const manualInstallDirs = ref<string[]>([]);

const editingGame = computed(() =>
  is_editing.value && editing_storage_key.value
    ? (config.value?.games.find(
        (candidate) => candidate.storage_key === editing_storage_key.value
      ) ?? null)
    : null
);

const activeSteamId = computed(
  () => pendingLudusaviMeta.value?.steamId ?? editingGame.value?.ludusavi_meta?.steamId ?? null
);
const activeStoreUserId = computed(() =>
  currentDevice.value
    ? (pendingStoreUserId.value ??
      editingGame.value?.store_user_ids?.[currentDevice.value.id] ??
      null)
    : pendingStoreUserId.value
);

// 获取当前设备信息
async function fetchCurrentDevice() {
  try {
    const result = await commands.getCurrentDeviceInfo();
    if (result.status === 'ok') {
      currentDevice.value = result.data;
    } else {
      notifyError(result.error);
    }
  } catch (e) {
    error(`Error getting current device info: ${e}`);
    notifyError($t('error.get_device_info_failed'));
  }
}
// 在组件挂载时获取当前设备信息
fetchCurrentDevice();

// init info when navigate from GameManage.vue
watchEffect(() => {
  const gameName = 'name' in route.params ? route.params.name : undefined;
  if (gameName) {
    const gameConfig = config.value?.games.find((game) => game.name === gameName);
    if (gameConfig) {
      is_editing.value = true;
      editing_storage_key.value = gameConfig.storage_key ?? '';
      game_name.value = gameConfig.name;
      save_paths.splice(0, save_paths.length, ...(gameConfig.save_paths ?? []));
      manualInstallDirs.value = [...(gameConfig.ludusavi_meta?.installDirs ?? [])];

      // 获取当前设备的游戏路径
      if (gameConfig.game_paths && currentDevice.value) {
        const deviceId = currentDevice.value.id;
        game_path.value = gameConfig.game_paths[deviceId] || '';
      } else {
        game_path.value = '';
      }
    } else {
      notifyError($t('addgame.change_target_not_exists_error') + gameName);
      router.back();
    }
  }
});

function check_save_unit_unique(p: string) {
  // 检查是否有任何存档单元的任何设备路径与新路径相同
  if (
    save_paths.find((x) => {
      if (!x.paths) return false;
      return Object.values(x.paths).includes(p);
    })
  ) {
    notifyWarning($t('addgame.duplicated_path_error'));
    return false;
  }
  return true;
}
function check_name_valid(name: string) {
  return name.trim().length > 0;
}
function generate_save_unit(
  unit_type: 'Folder' | 'File' | 'WinRegistry',
  path: string
): SaveUnitDraft {
  const delete_before_apply = config.value?.settings.default_delete_before_apply;

  // 创建一个基本的 SaveUnit，使用当前设备ID作为路径映射的键
  const saveUnit: SaveUnitDraft = {
    unit_type,
    paths: {},
    delete_before_apply,
    enabled: true,
  };

  // 如果有当前设备信息，则添加路径
  if (currentDevice.value) {
    const deviceId = currentDevice.value.id;
    saveUnit.paths![deviceId] = path;
  }

  return saveUnit;
}

async function add_save_directory() {
  try {
    const dir = await commands.chooseSaveDir();
    if (dir.status == 'error' || !check_save_unit_unique(dir.data)) {
      return;
    }
    save_paths.push(generate_save_unit('Folder', dir.data));
  } catch (e) {
    error(`Error choosing save directory: ${e}`);
    notifyError($t('error.choose_save_dir_error'));
  }
}

async function add_save_file() {
  try {
    const file = await commands.chooseSaveFile();
    if (file.status == 'error' || !check_save_unit_unique(file.data)) {
      return;
    }
    save_paths.push(generate_save_unit('File', file.data));
  } catch (e) {
    error(`Error choosing save file: ${e}`);
    notifyError($t('error.choose_save_file_error'));
  }
}

async function add_registry_key() {
  try {
    const result = await feedback.prompt(
      $t('addgame.registry_key_prompt'),
      $t('addgame.add_registry_key'),
      { inputPlaceholder: 'HKEY_CURRENT_USER\\SOFTWARE\\GameName' }
    );
    const path = result.value?.trim();
    if (!path) return;
    if (!check_save_unit_unique(path)) return;

    // Validate registry path on current platform
    const checkResult = await commands.checkPaths([path], null, null, null);
    if (checkResult.status === 'ok') {
      const [check] = checkResult.data;
      if (check && check.status === 'registryPath' && !check.supported) {
        notifyWarning($t('addgame.registry_non_windows_warning'));
      }
    }

    save_paths.push(generate_save_unit('WinRegistry', path));
  } catch {
    // dialog cancelled
  }
}

async function choose_executable_file() {
  try {
    const file = await commands.chooseSaveFile();
    if (file.status == 'error') {
      return;
    }
    game_path.value = file.data;
    if (manualInstallDirs.value.length === 0) {
      const segments = file.data.split(/[\\/]/).filter(Boolean);
      const inferred = segments.length >= 2 ? segments[segments.length - 2] : '';
      if (inferred) {
        manualInstallDirs.value = [inferred];
      }
    }
  } catch (e) {
    error(`Error choosing executable file: ${e}`);
    notifyError($t('error.choose_executable_file_error'));
  }
}

function submit_handler(button_method: () => void) {
  // 映射按钮的ID和他们要触发的方法
  button_method();
}

async function search_local() {
  try {
    importDialogLoading.value = true;
    showImportDialog.value = true;

    // Fetch games from ludusavi manifest (local games only by default)
    const result = await commands.fetchLudusaviGames(true);

    if (result.status === 'ok') {
      importableGames.value = result.data;
    } else {
      notifyError($t('game_import.fetch_error') + ': ' + result.error);
      showImportDialog.value = false;
    }
  } catch (e) {
    error(`Error fetching ludusavi games: ${e}`);
    notifyError($t('game_import.fetch_error'));
    showImportDialog.value = false;
  } finally {
    importDialogLoading.value = false;
  }
}

async function scan_vns() {
  const dirs = config.value?.settings.vn_scan_dirs;
  if (!dirs || dirs.length === 0) {
    notifyWarning($t('addgame.scan_vns_no_dirs'));
    return;
  }

  try {
    importDialogLoading.value = true;
    notifyInfo($t('addgame.scan_vns_scanning'));

    const result = await commands.scanVns(dirs);
    if (result.status === 'ok') {
      const drafts = result.data;
      if (drafts.length === 0) {
        notifyInfo($t('addgame.scan_vns_no_result'));
        return;
      }
      await openVnBatchImportDialog(drafts);
    } else {
      notifyError(result.error);
    }
  } catch (e) {
    error(`Error scanning visual novels: ${e}`);
    notifyError($t('game_import.fetch_error'));
  } finally {
    importDialogLoading.value = false;
  }
}

async function openVnBatchImportDialog(drafts: GameDraft[]) {
  const existingNames = new Set((config.value?.games ?? []).map((game) => game.name.toLowerCase()));

  const games: ImportableGame[] = [];
  const paths: Record<string, SavePath[]> = {};
  const deviceId = currentDevice.value?.id;

  for (const draft of drafts) {
    const savePaths: SavePath[] = [];

    for (const saveUnit of draft.save_paths) {
      const currentPath =
        (deviceId && saveUnit.paths?.[deviceId]) || Object.values(saveUnit.paths ?? {})[0] || '';
      if (!currentPath) {
        continue;
      }

      savePaths.push({
        path: currentPath,
        tags: [saveUnit.unit_type],
      });
    }

    games.push({
      name: draft.name,
      steamId: null,
      installDirs: [],
      isManaged: existingNames.has(draft.name.toLowerCase()),
      savePathsCount: savePaths.length,
    });
    paths[draft.name] = savePaths;
  }

  batchImportLoading.value = false;
  batchImportGames.value = games;
  batchGamePaths.value = paths;
  showBatchImportDialog.value = true;
}

async function handleLocalToggle(enabled: boolean) {
  try {
    importDialogLoading.value = true;

    // Refetch games with new filter setting
    const result = await commands.fetchLudusaviGames(enabled);

    if (result.status === 'ok') {
      importableGames.value = result.data;
    } else {
      notifyError($t('game_import.fetch_error') + ': ' + result.error);
    }
  } catch (e) {
    error(`Error toggling local filter: ${e}`);
    notifyError($t('game_import.fetch_error'));
  } finally {
    importDialogLoading.value = false;
  }
}

async function handleImportGames(selectedGames: ImportableGame[]) {
  if (selectedGames.length === 0) {
    return;
  }

  showImportDialog.value = false;

  if (selectedGames.length === 1) {
    // Show customization dialog for single game
    const firstGame = selectedGames[0];
    if (firstGame) {
      await showCustomizationDialog(firstGame);
    }
  } else {
    // Show batch import dialog for multiple games with editing capability
    await openBatchImportDialog(selectedGames);
  }
}

async function openBatchImportDialog(games: ImportableGame[]) {
  try {
    batchImportLoading.value = true;
    batchImportGames.value = games;
    batchGamePaths.value = {};
    showBatchImportDialog.value = true;

    // Fetch save paths for all selected games (parallel)
    const results = await Promise.allSettled(
      games.map((game) => commands.getGameSavePaths(game.name))
    );

    results.forEach((res, index) => {
      const game = games[index];
      if (!game) return;

      if (res.status === 'fulfilled') {
        const result = res.value;
        if (result.status === 'ok') {
          batchGamePaths.value[game.name] = result.data;
        } else {
          error(`Error fetching paths for ${game.name}: ${result.error}`);
        }
      } else {
        error(`Error fetching paths for ${game.name}: ${res.reason}`);
      }
    });
  } catch (e) {
    error(`Error preparing batch import: ${e}`);
    notifyError($t('game_import.fetch_error'));
    showBatchImportDialog.value = false;
  } finally {
    batchImportLoading.value = false;
  }
}

async function showCustomizationDialog(game: ImportableGame) {
  try {
    customizeDialogLoading.value = true;
    customizingGame.value = game;

    // Fetch detailed save paths for this game
    const result = await commands.getGameSavePaths(game.name);

    if (result.status === 'ok') {
      customizingSavePaths.value = result.data;
      showCustomizeDialog.value = true;
    } else {
      notifyError($t('game_import.fetch_error') + ': ' + result.error);
    }
  } catch (e) {
    error(`Error fetching game save paths: ${e}`);
    notifyError($t('game_import.fetch_error'));
  } finally {
    customizeDialogLoading.value = false;
  }
}

async function handleCustomizeConfirm(data: {
  gameName: string;
  savePaths: SavePath[];
  storeUserId: string | null;
}) {
  try {
    // Convert the customized data to our Game format
    const gameName = data.gameName || customizingGame.value?.name || '';

    // Filter out empty paths; registry paths are now supported
    const validPaths: string[] = [];
    const registryPaths: string[] = [];

    for (const sp of data.savePaths) {
      if (!sp.path || sp.path.trim() === '') {
        continue;
      }
      if (sp.path.startsWith('REGISTRY:') || sp.path.startsWith('HKEY_')) {
        registryPaths.push(sp.path);
      } else {
        validPaths.push(sp.path);
      }
    }

    if (validPaths.length === 0 && registryPaths.length === 0) {
      notifyWarning($t('game_import_customize.no_paths_selected'));
      return;
    }

    // Check non-registry paths with backend to determine file/folder type
    const pathInfoMap = new Map<string, PathCheckResult>();
    if (validPaths.length > 0) {
      const src = customizingGame.value;
      const checkResult = await commands.checkPaths(
        validPaths,
        data.storeUserId,
        src?.installDirs?.length ? src.installDirs : null,
        src?.steamId ?? null
      );
      if (checkResult.status === 'ok') {
        for (const info of checkResult.data) {
          pathInfoMap.set(info.rawPath, info);
        }
      }
    }

    // Build save units with accurate type info
    const savePaths: SaveUnitDraft[] = [];
    for (const path of validPaths) {
      const pathInfo = pathInfoMap.get(path);
      const isFile = pathInfo?.status === 'ok' ? pathInfo.isFile : undefined;
      const saveUnit: SaveUnitDraft = {
        unit_type: determineSaveUnitType(path, isFile),
        paths: {},
        delete_before_apply: config.value?.settings.default_delete_before_apply,
        enabled: true,
      };

      if (currentDevice.value) {
        saveUnit.paths![currentDevice.value.id] = path;
      }

      savePaths.push(saveUnit);
    }

    // Add registry save units
    for (const path of registryPaths) {
      const saveUnit: SaveUnitDraft = {
        unit_type: 'WinRegistry',
        paths: {},
        enabled: true,
      };

      if (currentDevice.value) {
        saveUnit.paths![currentDevice.value.id] = path;
      }

      savePaths.push(saveUnit);
    }

    // Set the game data in the form and save immediately (align with batch import)
    game_name.value = gameName;
    save_paths.splice(0, save_paths.length, ...savePaths);

    // Attach Ludusavi metadata if available from the importing game
    const src = customizingGame.value;
    if (src && (src.installDirs.length > 0 || src.steamId)) {
      pendingLudusaviMeta.value = {
        installDirs: src.installDirs,
        steamId: src.steamId ?? null,
      };
      manualInstallDirs.value = [...src.installDirs];
    }

    pendingStoreUserId.value = data.storeUserId;
    await save();
  } catch (e) {
    error(`Error importing game: ${e}`);
    notifyError($t('game_import.import_error'));
  }
}

interface GameConfig {
  name: string;
  customName: string;
  selected: boolean;
  paths: Array<{
    path: string;
    tags: string[];
    selected: boolean;
  }>;
}

async function handleBatchImportConfirm(configs: GameConfig[], storeUserId: string | null) {
  let successCount = 0;
  const failedGames: Array<{ name: string; reason: string }> = [];

  // Build lookup for Ludusavi metadata from original ImportableGame list
  const importableGameMap = new Map<string, ImportableGame>();
  for (const g of batchImportGames.value) {
    importableGameMap.set(g.name, g);
  }

  const existingNames = new Set(
    (config.value?.games ?? []).map((g) => (g.name ?? '').toLowerCase())
  );

  for (const gameConfig of configs) {
    try {
      const pathInfoMap = new Map<string, PathCheckResult>();
      // Get selected paths
      const selectedPaths = gameConfig.paths.filter((p) => p.selected);

      if (selectedPaths.length === 0) {
        continue;
      }

      const originalGame = importableGameMap.get(gameConfig.name);
      const nonRegistryPaths = selectedPaths
        .map((sp) => sp.path)
        .filter(
          (path): path is string =>
            !!path &&
            path.trim() !== '' &&
            !path.startsWith('REGISTRY:') &&
            !path.startsWith('HKEY_')
        );

      if (nonRegistryPaths.length > 0) {
        const checkResult = await commands.checkPaths(
          nonRegistryPaths,
          storeUserId,
          originalGame?.installDirs?.length ? originalGame.installDirs : null,
          originalGame?.steamId ?? null
        );
        if (checkResult.status === 'ok') {
          for (const info of checkResult.data) {
            pathInfoMap.set(info.rawPath, info);
          }
        }
      }

      // Convert to SaveUnits
      const savePaths: SaveUnitDraft[] = [];

      for (const sp of selectedPaths) {
        if (!sp.path || sp.path.trim() === '') {
          continue;
        }

        const isRegistry = sp.path.startsWith('REGISTRY:') || sp.path.startsWith('HKEY_');
        if (isRegistry) {
          const saveUnit: SaveUnitDraft = {
            unit_type: 'WinRegistry',
            paths: {},
            enabled: true,
          };
          if (currentDevice.value) {
            saveUnit.paths![currentDevice.value.id] = sp.path;
          }
          savePaths.push(saveUnit);
          continue;
        }

        const pathInfo = pathInfoMap.get(sp.path);
        const isFile = pathInfo?.status === 'ok' ? pathInfo.isFile : undefined;
        const saveUnit: SaveUnitDraft = {
          unit_type: determineSaveUnitType(sp.path, isFile),
          paths: {},
          delete_before_apply: config.value?.settings.default_delete_before_apply,
          enabled: true,
        };

        if (currentDevice.value) {
          saveUnit.paths![currentDevice.value.id] = sp.path;
        }

        savePaths.push(saveUnit);
      }

      // Skip if no valid paths
      if (savePaths.length === 0) {
        continue;
      }

      // Use custom name if provided, otherwise use original name
      const gameName = (gameConfig.customName || gameConfig.name).trim();
      if (!gameName || !check_name_valid(gameName)) {
        failedGames.push({ name: gameConfig.name, reason: $t('addgame.invalid_name_error') });
        continue;
      }
      const normalized = gameName.toLowerCase();
      if (existingNames.has(normalized)) {
        failedGames.push({ name: gameName, reason: $t('addgame.duplicated_name_error') });
        continue;
      }

      // Create the game with Ludusavi metadata if available
      const newGame: GameDraft = {
        name: gameName,
        save_paths: savePaths,
        store_user_ids: {},
        ludusavi_meta:
          originalGame && (originalGame.installDirs.length > 0 || originalGame.steamId)
            ? {
                installDirs: originalGame.installDirs,
                steamId: originalGame.steamId ?? null,
              }
            : undefined,
      };

      if (storeUserId && currentDevice.value) {
        newGame.store_user_ids = {};
        newGame.store_user_ids[currentDevice.value.id] = storeUserId;
      }

      const addResult = await commands.addGame(newGame);

      if (addResult.status === 'ok') {
        successCount++;
        existingNames.add(normalized);
        if (config.value && config.value.settings.add_new_to_favorites) {
          config.value.favorites = config.value.favorites ?? [];
          config.value.favorites.push({
            label: newGame.name,
            is_leaf: true,
            children: [],
            node_id: uuidv4().toString(),
          });
        }
      } else {
        failedGames.push({ name: gameName, reason: addResult.error });
      }
    } catch (e) {
      error(`Error importing game ${gameConfig.name}: ${e}`);
      failedGames.push({
        name: gameConfig.name,
        reason: e instanceof Error ? e.message : String(e),
      });
    }
  }

  if (config.value && config.value.settings.add_new_to_favorites) {
    try {
      await saveConfig();
    } catch (e) {
      error(`Error saving favorites after batch import: ${e}`);
    }
  }

  if (successCount > 0) {
    notifySuccess($t('game_import.import_success', { count: successCount }));
    await refreshConfig();
    if (failedGames.length > 0) {
      const failedDetails = failedGames.map((f) => `${f.name}: ${f.reason}`).join('\n');
      notifyWarning(
        $t('game_import.import_partial', { success: successCount, failed: failedGames.length }) +
          '\n' +
          failedDetails
      );
    }
  } else {
    notifyError($t('game_import.import_error'));
  }
}
async function save() {
  const storeUserId = pendingStoreUserId.value;
  const normalizedInstallDirs = manualInstallDirs.value
    .map((dir) => dir.trim())
    .filter((dir) => dir.length > 0);
  const steamId = activeSteamId.value;

  game_name.value = game_name.value.trim();
  if (game_name.value == '' || save_paths.length == 0) {
    notifyError($t('addgame.no_name_error'));
    return;
  }
  if (!check_name_valid(game_name.value)) {
    notifyError($t('addgame.invalid_name_error'));
    return;
  }

  // Duplicate name check: when editing, allow keeping the same name
  const duplicate = config.value?.games.find(
    (x) => x.name.toLowerCase() == game_name.value.toLowerCase()
  );
  if (duplicate) {
    if (!is_editing.value || duplicate.storage_key !== editing_storage_key.value) {
      notifyError($t('addgame.duplicated_name_error'));
      return;
    }
  }

  const game: GameDraft = {
    name: game_name.value,
    save_paths: save_paths,
    store_user_ids: { ...(editingGame.value?.store_user_ids ?? {}) },
    ludusavi_meta:
      normalizedInstallDirs.length > 0 || steamId !== null
        ? {
            installDirs: normalizedInstallDirs,
            steamId,
          }
        : undefined,
  };

  if (game_path.value && currentDevice.value) {
    game.game_paths = {};
    game.game_paths[currentDevice.value.id] = game_path.value;
  }
  if (currentDevice.value) {
    const currentDeviceId = currentDevice.value.id;
    if (storeUserId) {
      game.store_user_ids[currentDeviceId] = storeUserId;
    } else {
      game.store_user_ids = Object.fromEntries(
        Object.entries(game.store_user_ids).filter(([deviceId]) => deviceId !== currentDeviceId)
      );
    }
  }
  try {
    if (is_editing.value) {
      await commands.updateGame(editing_storage_key.value, game);
      is_editing.value = false;
      notifySuccess($t('addgame.add_game_success'));
      router.back();
    } else {
      await commands.addGame(game);
      if (config.value?.settings.add_new_to_favorites) {
        await refreshConfig();
        config.value?.favorites?.push({
          label: game.name,
          is_leaf: true,
          children: [],
          node_id: uuidv4().toString(),
        });
        await saveConfig();
      }
      notifySuccess($t('addgame.add_game_success'));
    }
    reset_info(false);
    await refreshConfig();
  } catch (e) {
    error(`Error saving game: ${e}`);
    notifyError($t('error.add_game_failed'));
  }
}
function reset_info(show_notification: boolean = true) {
  // 重置当前配置
  game_name.value = '';
  save_paths.splice(0, save_paths.length);
  game_path.value = '';
  manualInstallDirs.value = [];
  pendingLudusaviMeta.value = null;
  pendingStoreUserId.value = null;
  // TODO:This is a first occurrence of a i18n text duplication. How to handle this?
  if (show_notification) {
    notifySuccess($t('settings.reset_success'));
  }
}

function deleteRow(index: number) {
  save_paths.splice(index, 1);
}

// Helper function to determine save unit type from path
// If isFile is provided (from backend check_paths), use it; otherwise fallback to path heuristics
function determineSaveUnitType(
  path: string,
  isFile?: boolean | null
): 'File' | 'Folder' | 'WinRegistry' {
  // Registry paths become WinRegistry save units
  if (path.startsWith('REGISTRY:') || path.startsWith('HKEY_')) {
    return 'WinRegistry';
  }

  // If backend provided the answer, use it
  if (isFile !== undefined && isFile !== null) {
    return isFile ? 'File' : 'Folder';
  }

  // Paths ending with / are folders
  if (path.endsWith('/') || path.endsWith('\\')) {
    return 'Folder';
  }

  // Default to Folder for ambiguous cases (backend should provide accurate info)
  return 'Folder';
}
</script>

<template>
  <div class="page">
    <el-card class="form-card">
      <!-- Top: icon + fields -->
      <div class="top-row">
        <img class="game-icon" :src="game_icon_src" alt="" />

        <div class="fields">
          <el-form label-position="top" class="field-form">
            <el-form-item :label="$t('addgame.game_name')">
              <el-input v-model="game_name" :placeholder="$t('addgame.input_game_name_prompt')" />
            </el-form-item>

            <el-form-item :label="$t('addgame.game_launch_path')">
              <path-variable-input
                v-model="game_path"
                :show-status="true"
                :install-dirs="manualInstallDirs"
                :steam-id="activeSteamId"
                :store-user-id="activeStoreUserId"
              >
                <template #append>
                  <el-button text @click="choose_executable_file()">
                    <el-icon><DocumentAdd /></el-icon>
                  </el-button>
                </template>
              </path-variable-input>
            </el-form-item>

            <el-form-item :label="$t('addgame.install_dirs')">
              <el-select
                v-model="manualInstallDirs"
                multiple
                filterable
                allow-create
                default-first-option
                clearable
                collapse-tags
                collapse-tags-tooltip
                class="install-dirs-select"
                :placeholder="$t('addgame.install_dirs_placeholder')"
              />
              <div class="install-dirs-hint">
                {{ $t('addgame.install_dirs_hint') }}
              </div>
            </el-form-item>
          </el-form>
        </div>
      </div>
    </el-card>

    <el-card class="form-card">
      <!-- Toolbar -->
      <div class="table-toolbar">
        <div class="toolbar-left">
          <el-button type="primary" @click="add_save_directory">
            {{ $t('addgame.add_save_directory') }}
          </el-button>
          <el-button @click="add_save_file">
            {{ $t('addgame.add_save_file') }}
          </el-button>
          <el-button @click="add_registry_key">
            {{ $t('addgame.add_registry_key') }}
          </el-button>
        </div>
        <div class="toolbar-hint">
          <el-icon><InfoFilled /></el-icon>
          <span>{{ $t('addgame.path_variable_hint') }}</span>
        </div>
      </div>

      <!-- Save paths table -->
      <el-table :data="save_paths" style="width: 100%">
        <el-table-column prop="unit_type" :label="$t('addgame.type')" width="110" />
        <el-table-column :label="$t('addgame.path')" min-width="300">
          <template #default="scope">
            <path-variable-input
              :model-value="
                scope.row.paths && currentDevice ? scope.row.paths[currentDevice.id] || '' : ''
              "
              status-mode="below"
              :install-dirs="manualInstallDirs"
              :steam-id="activeSteamId"
              :store-user-id="activeStoreUserId"
              @update:model-value="
                (value) => {
                  if (currentDevice && scope.row.paths) {
                    scope.row.paths[currentDevice.id] = value;
                  }
                }
              "
            />
          </template>
        </el-table-column>
        <el-table-column v-if="currentDevice" :label="$t('addgame.device_info')" width="180">
          <template #default>
            <el-tag size="small" type="info">{{ currentDevice?.name }}</el-tag>
          </template>
        </el-table-column>
        <el-table-column :label="$t('addgame.operations')" width="100" align="center">
          <template #default="scope">
            <el-button link type="danger" size="small" @click.prevent="deleteRow(scope.$index)">
              {{ $t('addgame.remove') }}
            </el-button>
          </template>
        </el-table-column>
      </el-table>
    </el-card>

    <!-- Actions -->
    <div class="actions">
      <el-tooltip
        v-for="button in buttons"
        :key="button.text"
        :content="button.text"
        placement="top"
      >
        <el-button :type="button.type" circle @click="submit_handler(button.method)">
          <el-icon><component :is="button.icon" /></el-icon>
        </el-button>
      </el-tooltip>
    </div>

    <!-- Dialogs -->
    <game-import-dialog
      v-model="showImportDialog"
      :games="importableGames"
      :loading="importDialogLoading"
      @import="handleImportGames"
      @toggle-local="handleLocalToggle"
    />
    <game-import-customize-dialog
      v-model="showCustomizeDialog"
      :game-name="customizingGame?.name || ''"
      :save-paths="customizingSavePaths"
      :install-dirs="customizingGame?.installDirs || []"
      :steam-id="customizingGame?.steamId ?? null"
      :loading="customizeDialogLoading"
      @confirm="handleCustomizeConfirm"
    />
    <game-batch-import-dialog
      v-model="showBatchImportDialog"
      :games="batchImportGames"
      :game-paths="batchGamePaths"
      :loading="batchImportLoading"
      @confirm="handleBatchImportConfirm"
    />
  </div>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding-bottom: 24px;
}

.form-card {
  width: 100%;
}

.top-row {
  display: grid;
  grid-template-columns: 120px minmax(0, 1fr);
  gap: 20px;
  align-items: start;
}

.game-icon {
  flex-shrink: 0;
  width: 120px;
  height: 120px;
  border-radius: var(--el-border-radius-base);
  object-fit: cover;
  border: 1px solid var(--el-border-color-light);
}

.fields {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.field-form {
  margin: 0;
}

.field-form :deep(.el-form-item) {
  margin-bottom: 0;
}

.field-form :deep(.el-form-item + .el-form-item) {
  margin-top: 12px;
}

.install-dirs-select {
  width: 100%;
}

.install-dirs-hint {
  margin-top: 6px;
  font-size: 12px;
  color: var(--el-text-color-secondary);
}

.table-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 12px;
  margin-bottom: 16px;
}

.toolbar-left {
  display: flex;
  gap: 8px;
}

.toolbar-hint {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--el-text-color-secondary);
  max-width: 100%;
}

.toolbar-hint span {
  min-width: 0;
}

.page :deep(.el-table__body .el-table__cell) {
  vertical-align: top;
}

@media (max-width: 980px) {
  .top-row {
    grid-template-columns: 1fr;
  }

  .game-icon {
    width: 96px;
    height: 96px;
  }
}

.actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
