<script lang="ts" setup>
import { DocumentAdd, Check, RefreshRight, Download } from '@element-plus/icons-vue';
import { reactive, ref, watchEffect } from 'vue';
import {
  commands,
  type Game,
  type SaveUnit,
  type Device,
  type ImportableGame,
  type SavePath,
} from '../bindings';
import { $t } from '../i18n';
import { v4 as uuidv4 } from 'uuid';
import { error } from '@tauri-apps/plugin-log';
import PathVariableSelector from '../components/PathVariableSelector.vue';
import GameImportDialog from '../components/GameImportDialog.vue';
import GameImportCustomizeDialog from '../components/GameImportCustomizeDialog.vue';
import GameBatchImportDialog from '../components/GameBatchImportDialog.vue';

const route = useRoute();
const router = useRouter();
const { showError, showWarning, showSuccess } = useNotification();
const { config, refreshConfig, saveConfig } = useConfig();
const buttons = [
  {
    text: $t('addgame.search_local'),
    type: 'primary',
    icon: Download,
    method: search_local,
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
const save_paths = reactive<SaveUnit[]>([]); // 选择游戏存档目录
const game_path = ref(''); // 选择游戏启动程序
const game_icon_src = ref('/orange.png');
const is_editing = ref(false); // 是否正在编辑已有的游戏
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

// 获取当前设备信息
async function fetchCurrentDevice() {
  try {
    const result = await commands.getCurrentDeviceInfo();
    if (result.status === 'ok') {
      currentDevice.value = result.data;
      console.log('Current device:', currentDevice.value);
    } else {
      showError({ message: result.error });
    }
  } catch (e) {
    error(`Error getting current device info: ${e}`);
    showError({ message: $t('error.get_device_info_failed') });
  }
}
// 在组件挂载时获取当前设备信息
fetchCurrentDevice();

// init info when navigate from GameManage.vue
watchEffect(() => {
  const gameName = route.params.name;
  if (gameName) {
    const gameConfig = config.value?.games.find((game) => game.name === gameName);
    if (gameConfig) {
      is_editing.value = true;
      game_name.value = gameConfig.name;
      save_paths.splice(0, save_paths.length, ...(gameConfig.save_paths ?? []));

      // 获取当前设备的游戏路径
      if (gameConfig.game_paths && currentDevice.value) {
        const deviceId = currentDevice.value.id;
        game_path.value = gameConfig.game_paths[deviceId] || '';
      } else {
        game_path.value = '';
      }
    } else {
      showError({ message: $t('addgame.change_target_not_exists_error') + gameName });
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
    showWarning({ message: $t('addgame.duplicated_filename_error') });
    return false;
  }
  return true;
}
function check_name_valid(name: string) {
  const invalid_reg = /[<>:"/\\|?*]/;
  return !invalid_reg.test(name);
}
function generate_save_unit(unit_type: 'Folder' | 'File', path: string): SaveUnit {
  const delete_before_apply = config.value?.settings.default_delete_before_apply;

  // 创建一个基本的 SaveUnit，使用当前设备ID作为路径映射的键
  const saveUnit: SaveUnit = {
    unit_type,
    paths: {},
    delete_before_apply,
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
    showError({ message: $t('error.choose_save_dir_error') });
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
    showError({ message: $t('error.choose_save_file_error') });
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
    showError({ message: $t('error.choose_executable_file_error') });
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
      showError({ message: $t('game_import.fetch_error') + ': ' + result.error });
      showImportDialog.value = false;
    }
  } catch (e) {
    error(`Error fetching ludusavi games: ${e}`);
    showError({ message: $t('game_import.fetch_error') });
    showImportDialog.value = false;
  } finally {
    importDialogLoading.value = false;
  }
}

async function handleLocalToggle(enabled: boolean) {
  try {
    importDialogLoading.value = true;

    // Refetch games with new filter setting
    const result = await commands.fetchLudusaviGames(enabled);

    if (result.status === 'ok') {
      importableGames.value = result.data;
    } else {
      showError({ message: $t('game_import.fetch_error') + ': ' + result.error });
    }
  } catch (e) {
    error(`Error toggling local filter: ${e}`);
    showError({ message: $t('game_import.fetch_error') });
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
    showError({ message: $t('game_import.fetch_error') });
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
      showError({ message: $t('game_import.fetch_error') + ': ' + result.error });
    }
  } catch (e) {
    error(`Error fetching game save paths: ${e}`);
    showError({ message: $t('game_import.fetch_error') });
  } finally {
    customizeDialogLoading.value = false;
  }
}

async function handleCustomizeConfirm(data: { gameName: string; savePaths: SavePath[] }) {
  try {
    // Convert the customized data to our Game format
    const gameName = data.gameName || customizingGame.value?.name || '';

    // Filter and map save paths, removing empty ones
    const validSavePaths: SaveUnit[] = [];
    let skippedRegistry = 0;

    for (const sp of data.savePaths) {
      // Skip empty or whitespace-only paths
      if (!sp.path || sp.path.trim() === '') {
        continue;
      }
      if (sp.path.startsWith('REGISTRY:') || sp.path.startsWith('HKEY_')) {
        skippedRegistry++;
        continue;
      }

      const saveUnit: SaveUnit = {
        unit_type: determineSaveUnitType(sp.path),
        paths: {},
        delete_before_apply: config.value?.settings.default_delete_before_apply,
      };

      // Add path for current device
      if (currentDevice.value) {
        saveUnit.paths![currentDevice.value.id] = sp.path;
      }

      validSavePaths.push(saveUnit);
    }

    // Validate that we have at least one valid path
    if (validSavePaths.length === 0) {
      showWarning({ message: $t('game_import_customize.no_paths_selected') });
      return;
    }

    if (skippedRegistry > 0) {
      showWarning({ message: $t('game_import.registry_skipped', { count: skippedRegistry }) });
    }

    // Set the game data in the form and save immediately (align with batch import)
    game_name.value = gameName;
    save_paths.splice(0, save_paths.length, ...validSavePaths);

    await save();
  } catch (e) {
    error(`Error importing game: ${e}`);
    showError({ message: $t('game_import.import_error') });
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

async function handleBatchImportConfirm(configs: GameConfig[]) {
  let successCount = 0;
  let skippedRegistryCount = 0;
  let failedCount = 0;
  const existingNames = new Set(
    (config.value?.games ?? []).map((g) => (g.name ?? '').toLowerCase())
  );

  for (const gameConfig of configs) {
    try {
      // Get selected paths
      const selectedPaths = gameConfig.paths.filter((p) => p.selected);

      if (selectedPaths.length === 0) {
        continue;
      }

      // Convert to SaveUnits
      const savePaths: SaveUnit[] = [];

      for (const sp of selectedPaths) {
        if (!sp.path || sp.path.trim() === '') {
          continue;
        }
        if (sp.path.startsWith('REGISTRY:') || sp.path.startsWith('HKEY_')) {
          skippedRegistryCount++;
          continue;
        }

        const saveUnit: SaveUnit = {
          unit_type: determineSaveUnitType(sp.path),
          paths: {},
          delete_before_apply: config.value?.settings.default_delete_before_apply,
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
        failedCount++;
        continue;
      }
      const normalized = gameName.toLowerCase();
      if (existingNames.has(normalized)) {
        failedCount++;
        continue;
      }

      // Create the game
      const newGame: Game = {
        name: gameName,
        save_paths: savePaths,
      };

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
        failedCount++;
      }
    } catch (e) {
      error(`Error importing game ${gameConfig.name}: ${e}`);
      failedCount++;
    }
  }

  if (config.value && config.value.settings.add_new_to_favorites) {
    try {
      await saveConfig();
    } catch (e) {
      error(`Error saving favorites after batch import: ${e}`);
    }
  }

  if (skippedRegistryCount > 0) {
    showWarning({ message: $t('game_import.registry_skipped', { count: skippedRegistryCount }) });
  }

  if (successCount > 0) {
    showSuccess({ message: $t('game_import.import_success', { count: successCount }) });
    await refreshConfig();
    if (failedCount > 0) {
      showWarning({ message: $t('game_import.import_partial', { success: successCount, failed: failedCount }) });
    }
  } else {
    showError({ message: $t('game_import.import_error') });
  }
}
async function save() {
  // 去除头尾空字符，防止触发Windows文件命名规则问题
  game_name.value = game_name.value.trim();
  if (game_name.value == '' || save_paths.length == 0) {
    showError({ message: $t('addgame.no_name_error') });
    return;
  }
  if (!check_name_valid(game_name.value)) {
    showError({ message: $t('addgame.invalid_name_error') });
    return;
  }
  if (config.value?.games.find((x) => x.name.toLowerCase() == game_name.value.toLowerCase())) {
    showError({ message: $t('addgame.duplicated_name_error') });
    return;
  }
  const game: Game = {
    name: game_name.value,
    save_paths: save_paths,
  };

  // 如果有游戏路径和当前设备信息，则添加游戏路径
  if (game_path.value && currentDevice.value) {
    game.game_paths = {};
    game.game_paths[currentDevice.value.id] = game_path.value;
  }
  try {
    await commands.addGame(game);

    if (is_editing.value) {
      is_editing.value = false;
      showSuccess({ message: $t('addgame.add_game_success') });
      router.back();
    } else {
      if (config.value?.settings.add_new_to_favorites) {
        // TODO:以下内容是否需要抽离成单独的工具库？还是说应该后端处理？
        await refreshConfig();
        config.value?.favorites?.push({
          label: game.name,
          is_leaf: true,
          children: [],
          node_id: uuidv4().toString(),
        });
        await saveConfig();
      }
      showSuccess({ message: $t('addgame.add_game_success') });
    }
    reset_info(false);
    await refreshConfig();
  } catch (e) {
    error(`Error adding game: ${e}`);
    showError({ message: $t('error.add_game_failed') });
  }
}
function reset_info(show_notification: boolean = true) {
  // 重置当前配置
  game_name.value = '';
  save_paths.splice(0, save_paths.length);
  game_path.value = '';
  // TODO:This is a first occurrence of a i18n text duplication. How to handle this?
  if (show_notification) {
    showSuccess({ message: $t('settings.reset_success') });
  }
}

function deleteRow(index: number) {
  save_paths.splice(index, 1);
}

// Helper function to determine save unit type from path
function determineSaveUnitType(path: string): 'File' | 'Folder' {
  // Registry paths are treated as files, but are currently skipped during import
  if (path.startsWith('REGISTRY:') || path.startsWith('HKEY_')) {
    return 'File';
  }

  // Paths ending with / are folders
  if (path.endsWith('/') || path.endsWith('\\')) {
    return 'Folder';
  }

  // Paths with common file extensions are files
  const fileExtensions = ['.sav', '.dat', '.cfg', '.ini', '.xml', '.json', '.txt', '.bin'];
  const lowerPath = path.toLowerCase();
  if (fileExtensions.some((ext) => lowerPath.endsWith(ext))) {
    return 'File';
  }

  // Default to Folder for ambiguous cases
  return 'Folder';
}

</script>

<template>
  <div class="select-container">
    <el-card class="game-info">
      <div class="top-part">
        <img class="game-icon" :src="game_icon_src" />
        <div class="bold">
          {{ $t('addgame.warning_for_save_file') }}
        </div>
        <el-input
          v-model="game_name"
          :placeholder="$t('addgame.input_game_name_prompt')"
          class="game-name"
        >
          <template #prepend> {{ $t('addgame.game_name') }} </template>
        </el-input>
        <el-input
          v-model="game_path"
          :placeholder="$t('addgame.input_game_launch_path_prompt')"
          class="game-path"
        >
          <template #prepend> {{ $t('addgame.game_launch_path') }} </template>
          <template #append>
            <el-button @click="choose_executable_file()">
              <el-icon>
                <document-add />
              </el-icon>
            </el-button>
          </template>
        </el-input>
      </div>
      <div class="add-button-area">
        <div class="button-row">
          <el-button type="primary" @click="add_save_directory">{{
            $t('addgame.add_save_directory')
          }}</el-button>
          <el-button type="primary" @click="add_save_file">{{
            $t('addgame.add_save_file')
          }}</el-button>
        </div>
        <div class="path-variable-info">
          <el-alert type="info" :closable="false" show-icon>
            {{ $t('addgame.path_variable_hint') }}
          </el-alert>
        </div>
      </div>
      <el-table :data="save_paths" class="save-table">
        <el-table-column fixed prop="unit_type" :label="$t('addgame.type')" width="120" />
        <el-table-column :label="$t('addgame.operations')" width="120">
          <template #default="scope">
            <el-button link type="primary" size="small" @click.prevent="deleteRow(scope.$index)">
              {{ $t('addgame.remove') }}
            </el-button>
          </template>
        </el-table-column>
        <el-table-column :label="$t('addgame.path')" min-width="300">
          <template #default="scope">
            <div class="path-input-container">
              <el-input
                :model-value="
                  scope.row.paths && currentDevice ? scope.row.paths[currentDevice.id] || '' : ''
                "
                size="small"
                @update:model-value="
                  (value) => {
                    if (currentDevice && scope.row.paths) {
                      scope.row.paths[currentDevice.id] = value;
                    }
                  }
                "
              >
                <template #append>
                  <path-variable-selector
                    :current-path="
                      scope.row.paths && currentDevice
                        ? scope.row.paths[currentDevice.id] || ''
                        : ''
                    "
                    @insert="
                      (variable) => {
                        if (currentDevice && scope.row.paths) {
                          const currentPath = scope.row.paths[currentDevice.id] || '';
                          scope.row.paths[currentDevice.id] = currentPath + variable;
                        }
                      }
                    "
                  />
                </template>
              </el-input>
            </div>
          </template>
        </el-table-column>
        <el-table-column v-if="currentDevice" :label="$t('addgame.device_info')" width="200">
          <template #default>
            <el-tag size="small">{{ currentDevice?.name }}</el-tag>
          </template>
        </el-table-column>
      </el-table>
    </el-card>
    <el-container class="submit-bar">
      <el-tooltip
        v-for="button in buttons"
        :key="button.text"
        :content="button.text"
        placement="top"
      >
        <el-button :type="button.type" circle @click="submit_handler(button.method)">
          <el-icon>
            <component :is="button.icon" />
          </el-icon>
        </el-button>
      </el-tooltip>
    </el-container>

    <!-- Game Import Dialog -->
    <game-import-dialog
      v-model="showImportDialog"
      :games="importableGames"
      :loading="importDialogLoading"
      @import="handleImportGames"
      @toggle-local="handleLocalToggle"
    />

    <!-- Game Customize Dialog (single game) -->
    <game-import-customize-dialog
      v-model="showCustomizeDialog"
      :game-name="customizingGame?.name || ''"
      :save-paths="customizingSavePaths"
      :loading="customizeDialogLoading"
      @confirm="handleCustomizeConfirm"
    />

    <!-- Batch Import Dialog (multiple games) -->
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
.bold {
  margin-left: 10px;
  font-weight: bold;
  color: var(--el-text-color-primary);
}

.save-table {
  margin-top: 20px;
  margin-bottom: 20px;
}

.select-container {
  height: 90%;
  width: 100%;
}

.el-card {
  margin-bottom: 15px;
  padding-bottom: 20px;
}

.top-part {
  height: 200px;
  display: grid;
  grid-template-columns: 1fr 3fr;
  grid-template-rows: 1fr 1fr 1fr 1fr 1fr 1fr;
}

.top-part > img {
  grid-column: 1/2;
  grid-row: 1/7;
  margin: auto;
}

.game-name {
  grid-column: 2/3;
  grid-row: 5/6;
  margin-bottom: 5px;
}

.game-path {
  grid-column: 2/3;
  grid-row: 6/7;
}

.game-icon {
  float: left;
  height: 200px;
  width: 200px;
}

.add-button-area {
  margin-top: 20px;
}

.button-row {
  display: flex;
  gap: 10px;
  margin-bottom: 10px;
}

.path-variable-info {
  margin-top: 10px;
  margin-bottom: 10px;
}

.path-input-container {
  display: flex;
  align-items: center;
}

.submit-bar {
  justify-content: flex-end;
  height: 10%;
}
</style>
