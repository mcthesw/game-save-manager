<script lang="ts" setup>
import { Check, Download, FilePlus2, RotateCcw, Search, Trash2 } from '@lucide/vue';
import { computed, reactive, ref, watch } from 'vue';
import {
  commands,
  type GameDraft,
  type SaveUnitDraft,
  type Device,
  type ImportableGame,
  type SavePath,
  type PathCheckResult,
} from '../api/commands';
import { $t } from '../i18n';
import { v4 as uuidv4 } from 'uuid';
import { error } from '../utils/logger';
import PathVariableInput from './PathVariableInput.vue';
import GameImportDialog from './GameImportDialog.vue';
import GameImportCustomizeDialog from './GameImportCustomizeDialog.vue';
import GameBatchImportDialog from './GameBatchImportDialog.vue';
import { KAlert, KButton, KDrawer, KInput, KTag, KTagInput } from '../ui/kit';
import { concreteSaveUnit, manifestSaveUnit, saveUnitPaths, saveUnitType } from '../utils/saveUnit';
import { useAddGameDrawer } from '../composables/useAddGameDrawer';

const feedback = useFeedback();
const { config, refreshConfig, saveConfig } = useConfig();
const { visible, editGameName, close } = useAddGameDrawer();

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
  () =>
    pendingLudusaviMeta.value?.steamId ??
    (Number(
      editingGame.value?.ludusavi_meta?.storeGameIds?.find((entry) => entry.store === 'steam')?.id
    ) ||
      null)
);
const activeStoreUserId = computed(() => pendingStoreUserId.value);
const needsInstallDirectoryNames = computed(
  () =>
    manualInstallDirs.value.length > 0 ||
    save_paths.some(
      (unit) =>
        unit.source.type === 'manifestPattern' &&
        (unit.source.pattern.includes('<game>') || unit.source.pattern.includes('<base>'))
    )
);

async function ensureSteamAccountResource(userId: string | null): Promise<number | null> {
  if (!userId || !currentDevice.value || !config.value) return null;
  currentDevice.value.resources ??= [];
  const existing = currentDevice.value.resources.find(
    (resource) =>
      resource.kind.type === 'storeAccount' &&
      resource.kind.store === 'steam' &&
      resource.kind.user_id === userId
  );
  if (existing) return existing.id;
  const id = currentDevice.value.next_resource_id ?? 0;
  currentDevice.value.resources.push({
    id,
    source: 'manual',
    kind: { type: 'storeAccount', store: 'steam', user_id: userId },
  });
  currentDevice.value.next_resource_id = id + 1;
  config.value.devices ??= {};
  config.value.devices[currentDevice.value.id] = currentDevice.value;
  await saveConfig();
  return id;
}

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

// 抽屉每次打开时初始化表单：编辑模式装载既有游戏，创建模式清空
watch(visible, (isOpen) => {
  if (!isOpen) return;
  const gameName = editGameName.value;
  if (!gameName) {
    is_editing.value = false;
    editing_storage_key.value = '';
    reset_info(false);
    return;
  }
  const gameConfig = config.value?.games.find((game) => game.name === gameName);
  if (gameConfig) {
    is_editing.value = true;
    editing_storage_key.value = gameConfig.storage_key ?? '';
    game_name.value = gameConfig.name;
    save_paths.splice(0, save_paths.length, ...(gameConfig.save_paths ?? []));
    manualInstallDirs.value = [...(gameConfig.ludusavi_meta?.installDirs ?? [])];
    pendingLudusaviMeta.value = null;
    pendingStoreUserId.value = null;

    // 获取当前设备的游戏路径
    if (gameConfig.game_paths && currentDevice.value) {
      const deviceId = currentDevice.value.id;
      game_path.value = gameConfig.game_paths[deviceId] || '';
    } else {
      game_path.value = '';
    }
  } else {
    notifyError($t('addgame.change_target_not_exists_error') + gameName);
    close();
  }
});

function check_save_unit_unique(p: string) {
  // 检查是否有任何存档单元的任何设备路径与新路径相同
  if (
    save_paths.find((x) => {
      const paths = saveUnitPaths(x);
      return paths ? Object.values(paths).includes(p) : false;
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
  const saveUnit = concreteSaveUnit(
    unit_type,
    {},
    {
      delete_before_apply,
      enabled: true,
    }
  );

  // 如果有当前设备信息，则添加路径
  if (currentDevice.value) {
    const deviceId = currentDevice.value.id;
    if (saveUnit.source.type === 'concrete') {
      saveUnit.source.paths![deviceId] = path;
    }
  }

  return saveUnit;
}

function saveUnitDisplayPath(row: SaveUnitDraft): string {
  const unit = row;
  if (unit.source.type === 'manifestPattern') return unit.source.pattern;
  if (!currentDevice.value) return Object.values(unit.source.paths ?? {})[0] ?? '';
  return unit.source.paths?.[currentDevice.value.id] ?? '';
}

function updateSaveUnitPath(row: SaveUnitDraft, value: string) {
  const unit = row;
  if (unit.source.type === 'manifestPattern') {
    unit.source.pattern = value;
  } else if (currentDevice.value) {
    unit.source.paths ??= {};
    unit.source.paths[currentDevice.value.id] = value;
  }
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
  } catch (e) {
    error(`Error choosing executable file: ${e}`);
    notifyError($t('error.choose_executable_file_error'));
  }
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
      const paths = saveUnitPaths(saveUnit);
      const currentPath = (deviceId && paths?.[deviceId]) || Object.values(paths ?? {})[0] || '';
      if (!currentPath) {
        continue;
      }

      savePaths.push({
        path: currentPath,
        tags: [saveUnitType(saveUnit) ?? 'Folder'],
        constraints: { alternatives: [] },
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
      const saveUnit = manifestSaveUnit(
        path,
        {
          delete_before_apply: config.value?.settings.default_delete_before_apply,
          enabled: true,
        },
        data.savePaths.find((entry) => entry.path === path)?.constraints ?? { alternatives: [] }
      );

      savePaths.push(saveUnit);
    }

    // Add registry save units
    for (const path of registryPaths) {
      const saveUnit = concreteSaveUnit('WinRegistry', {}, { enabled: true });

      if (currentDevice.value && saveUnit.source.type === 'concrete') {
        saveUnit.source.paths![currentDevice.value.id] = path;
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
    constraints?: SavePath['constraints'];
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
  const accountResourceId = await ensureSteamAccountResource(storeUserId);

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
          const saveUnit = concreteSaveUnit('WinRegistry', {}, { enabled: true });
          if (currentDevice.value && saveUnit.source.type === 'concrete') {
            saveUnit.source.paths![currentDevice.value.id] = sp.path;
          }
          savePaths.push(saveUnit);
          continue;
        }

        const saveUnit = manifestSaveUnit(
          sp.path,
          {
            delete_before_apply: config.value?.settings.default_delete_before_apply,
            enabled: true,
          },
          sp.constraints ?? { alternatives: [] }
        );

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
        device_bindings: {},
        ludusavi_meta:
          originalGame && (originalGame.installDirs.length > 0 || originalGame.steamId)
            ? {
                installDirs: originalGame.installDirs,
                storeGameIds: originalGame.steamId
                  ? [{ store: 'steam', id: String(originalGame.steamId) }]
                  : [],
              }
            : undefined,
      };
      if (accountResourceId !== null && currentDevice.value) {
        newGame.device_bindings![currentDevice.value.id] = {
          accountIds: [accountResourceId],
          restoreMappings: [],
        };
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
  const accountResourceId = await ensureSteamAccountResource(pendingStoreUserId.value);
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
    device_bindings: { ...(editingGame.value?.device_bindings ?? {}) },
    ludusavi_meta:
      normalizedInstallDirs.length > 0 || steamId !== null
        ? {
            installDirs: normalizedInstallDirs,
            storeGameIds: steamId ? [{ store: 'steam', id: String(steamId) }] : [],
          }
        : undefined,
  };
  if (accountResourceId !== null && currentDevice.value) {
    game.device_bindings ??= {};
    const existingBinding = game.device_bindings[currentDevice.value.id];
    game.device_bindings[currentDevice.value.id] = {
      ...existingBinding,
      accountIds: [accountResourceId],
      restoreMappings: existingBinding?.restoreMappings ?? [],
    };
  }

  if (game_path.value && currentDevice.value) {
    game.game_paths = {};
    game.game_paths[currentDevice.value.id] = game_path.value;
  }
  try {
    if (is_editing.value) {
      await commands.updateGame(editing_storage_key.value, game);
      is_editing.value = false;
      notifySuccess($t('addgame.add_game_success'));
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
    close();
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
  if (show_notification) {
    notifySuccess($t('settings.reset_success'));
  }
}

function deleteRow(index: number) {
  save_paths.splice(index, 1);
}
</script>

<template>
  <KDrawer
    v-model:open="visible"
    :title="is_editing ? $t('addgame.drawer_title_edit') : $t('addgame.drawer_title_add')"
    :width="700"
  >
    <div class="flex flex-col gap-6">
      <!-- 基本信息 -->
      <section class="flex gap-4">
        <img
          class="h-14 w-14 shrink-0 rounded-md border border-border object-cover"
          :src="game_icon_src"
          alt=""
        />
        <div class="flex min-w-0 flex-1 flex-col gap-4">
          <div>
            <label class="mb-1 block text-xs text-text-dim">{{ $t('addgame.game_name') }}</label>
            <KInput v-model="game_name" :placeholder="$t('addgame.input_game_name_prompt')" />
          </div>
          <div>
            <label class="mb-1 block text-xs text-text-dim">{{
              $t('addgame.game_launch_path')
            }}</label>
            <PathVariableInput
              v-model="game_path"
              :show-status="true"
              :install-dirs="manualInstallDirs"
              :steam-id="activeSteamId"
              :store-user-id="activeStoreUserId"
            >
              <template #append>
                <KButton
                  variant="ghost"
                  size="sm"
                  :aria-label="$t('addgame.input_game_launch_path_prompt')"
                  @click="choose_executable_file()"
                >
                  <FilePlus2 :size="14" aria-hidden="true" />
                </KButton>
              </template>
            </PathVariableInput>
          </div>
          <div v-if="needsInstallDirectoryNames">
            <label class="mb-1 block text-xs text-text-dim">{{ $t('addgame.install_dirs') }}</label>
            <KTagInput
              v-model="manualInstallDirs"
              :placeholder="$t('addgame.install_dirs_placeholder')"
            />
            <p class="mt-1 text-xs leading-relaxed text-text-dim">
              {{ $t('addgame.install_dirs_hint') }}
            </p>
          </div>
        </div>
      </section>

      <!-- 存档位置 -->
      <section>
        <div class="mb-2 flex flex-wrap items-center gap-2">
          <span class="text-sm font-medium text-text">{{ $t('addgame.save_paths_section') }}</span>
          <div class="ml-auto flex flex-wrap items-center gap-1.5">
            <KButton size="sm" variant="primary" @click="add_save_directory">
              {{ $t('addgame.add_save_directory') }}
            </KButton>
            <KButton size="sm" @click="add_save_file">
              {{ $t('addgame.add_save_file') }}
            </KButton>
            <KButton size="sm" @click="add_registry_key">
              {{ $t('addgame.add_registry_key') }}
            </KButton>
          </div>
        </div>
        <KAlert tone="info" class="mb-3">{{ $t('addgame.path_variable_hint') }}</KAlert>

        <div class="rounded-sm border border-border">
          <div
            v-for="(row, index) in save_paths"
            :key="index"
            class="flex items-center gap-2 border-b border-border px-3 py-2 last:border-b-0"
          >
            <KTag class="w-20 shrink-0 justify-center text-center">
              {{
                row.source.type === 'manifestPattern'
                  ? $t('addgame.dynamic_path')
                  : row.source.unit_type
              }}
            </KTag>
            <div class="min-w-0 flex-1">
              <PathVariableInput
                :model-value="saveUnitDisplayPath(row)"
                status-mode="below"
                :install-dirs="manualInstallDirs"
                :steam-id="activeSteamId"
                :store-user-id="activeStoreUserId"
                @update:model-value="(value: string) => updateSaveUnitPath(row, value)"
              />
            </div>
            <KTag v-if="currentDevice" class="shrink-0">{{ currentDevice.name }}</KTag>
            <KButton
              variant="ghost"
              size="sm"
              :aria-label="$t('addgame.remove')"
              class="shrink-0 text-danger hover:bg-danger-soft"
              @click="deleteRow(index)"
            >
              <Trash2 :size="14" aria-hidden="true" />
            </KButton>
          </div>
          <div v-if="save_paths.length === 0" class="px-3 py-6 text-center text-sm text-text-dim">
            {{ $t('addgame.no_save_paths') }}
          </div>
        </div>
      </section>
    </div>

    <template #footer>
      <KButton size="sm" @click="search_local()">
        <Download :size="13" aria-hidden="true" />
        {{ $t('addgame.search_local') }}
      </KButton>
      <KButton size="sm" @click="scan_vns()">
        <Search :size="13" aria-hidden="true" />
        {{ $t('addgame.scan_vns') }}
      </KButton>
      <KButton size="sm" variant="ghost" @click="reset_info()">
        <RotateCcw :size="13" aria-hidden="true" />
        {{ $t('addgame.reset_current_profile') }}
      </KButton>
      <div class="flex-1" />
      <KButton variant="primary" @click="save()">
        <Check :size="14" aria-hidden="true" />
        {{ $t('common.save') }}
      </KButton>
    </template>
  </KDrawer>

  <!-- 导入流程对话框（teleport 到 body，与抽屉层级互不干扰） -->
  <GameImportDialog
    v-model="showImportDialog"
    :games="importableGames"
    :loading="importDialogLoading"
    @import="handleImportGames"
    @toggle-local="handleLocalToggle"
  />
  <GameImportCustomizeDialog
    v-model="showCustomizeDialog"
    :game-name="customizingGame?.name || ''"
    :save-paths="customizingSavePaths"
    :install-dirs="customizingGame?.installDirs || []"
    :steam-id="customizingGame?.steamId ?? null"
    :loading="customizeDialogLoading"
    @confirm="handleCustomizeConfirm"
  />
  <GameBatchImportDialog
    v-model="showBatchImportDialog"
    :games="batchImportGames"
    :game-paths="batchGamePaths"
    :loading="batchImportLoading"
    @confirm="handleBatchImportConfirm"
  />
</template>
