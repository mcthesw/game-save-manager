<script lang="ts" setup>
import { computed, ref, watch, onBeforeUnmount, onMounted } from 'vue';
import { ElInput } from 'element-plus';
import { useRoute, useRouter } from 'vue-router';
import { commands, events } from '../../bindings';
import SaveLocationDrawer from '../../components/SaveLocationDrawer.vue';
import BranchTreeView from '../../components/BranchTreeView.vue';
import ExtraBackupDrawer from '../../components/ExtraBackupDrawer.vue';
import type { Game, Snapshot, Device, GameSnapshots } from '../../bindings';
import { $t } from '../../i18n';
import { error, info } from '@tauri-apps/plugin-log';
import {
  List,
  Share,
  VideoPlay,
  Folder,
  DocumentCopy,
  Setting,
  Delete,
  Back,
  Plus,
  Timer,
  Edit,
} from '@element-plus/icons-vue';
import dayjs from 'dayjs';

const { showInfo, showError, showSuccess } = useNotification();
const feedback = useFeedback();
const { config, refreshConfig, saveConfig } = useConfig();
const { withLoading } = useGlobalLoading();
const router = useRouter();
const route = useRoute();

// View mode: 'table' or 'branch'
const viewMode = ref<'table' | 'branch'>('table');

const search = ref(''); // 搜索时使用的字符串
const drawer = ref(false); // 是否显示存档位置侧栏
const extraBackupDrawer = ref(false); // 是否显示额外备份抽屉

const table_data = ref<Snapshot[]>([]);

// Game snapshots info including HEAD
const gameSnapshots = ref<GameSnapshots | null>(null);

const game: Ref<Game> = ref({
  name: '',
  save_paths: [],
  game_paths: {},
});

// 当前设备信息
const currentDevice = ref<Device | null>(null);

// 获取当前设备信息
async function fetchCurrentDevice() {
  try {
    const result = await commands.getCurrentDeviceInfo();
    if (result.status === 'ok') {
      currentDevice.value = result.data;
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

const describe = ref('');
let backup_button_time_limit = true; // 两次备份时间间隔1秒
let backup_button_backup_limit = true; // 上次没备份好禁止再备份或读取
let apply_button_apply_limit = true; // 上次未恢复好禁止读取或备份

// 撤销上次应用的状态
interface UndoInfo {
  extraBackupDate: string;
  previousHead: string | null;
}
const undoInfo = ref<UndoInfo | null>(null);

// 撤销按钮是否可用
const canUndo = computed(() => undoInfo.value !== null);
const extraBackupEnabled = computed(() => config.value.settings.extra_backup_when_apply !== false);
const undoTooltip = computed(() => {
  if (!extraBackupEnabled.value) return $t('manage.undo_requires_extra_backup');
  if (!canUndo.value) return $t('manage.undo_not_available');
  return $t('manage.undo_last_apply');
});

// 批量操作记录列表
const selected_game_snapshots: Ref<Snapshot[]> = ref([]);

let stopQuickActionListener: (() => void) | null = null;

onMounted(async () => {
  try {
    stopQuickActionListener = await events.quickActionCompleted.listen(async (event) => {
      const payload = event.payload;
      if (
        payload.status === 'Success' &&
        payload.operation === 'Backup' &&
        payload.game_name &&
        payload.game_name === game.value.name
      ) {
        await refresh_backups_info();
      }
    });
  } catch (e) {
    error(`Failed to listen quick action events: ${e}`);
  }
});

onBeforeUnmount(() => {
  if (stopQuickActionListener) {
    stopQuickActionListener();
    stopQuickActionListener = null;
  }
});

// 格式化文件大小显示
function formatFileSize(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}
function on_selection_change(val: Snapshot[]) {
  selected_game_snapshots.value = val;
}
async function batch_delete() {
  try {
    const promptResult = await feedback.prompt($t('manage.batch_delete_prompt'), $t('home.hint'), {
      confirmButtonText: $t('manage.confirm'),
      cancelButtonText: $t('manage.cancel'),
      inputPattern: /yes/,
      inputErrorMessage: $t('manage.invalid_input_error'),
    });

    if (promptResult.value === 'yes') {
      const dates = selected_game_snapshots.value.map((item) => item.date);
      const deleteResult = await commands.batchDeleteSnapshots(game.value, dates);
      await refresh_backups_info();
      if (deleteResult.status === 'ok') {
        showSuccess({ message: $t('manage.batch_delete_success', { count: dates.length }) });
      } else {
        showError({ message: deleteResult.error });
      }
    } else {
      showInfo({ message: $t('manage.invalid_input_error') });
    }
  } catch {
    showError({ message: $t('manage.operation_canceled') });
  }
}

// Init game info
watch(
  () => route.params.name,
  (newValue) => {
    if (!newValue) {
      return;
    }
    const name = newValue;
    game.value = config.value.games.find((x) => x.name == name) as Game;
    refresh_backups_info();
    // 检查当前设备的存档路径是否为空
    checkCurrentDeviceSavePaths();
  },
  { immediate: true }
);

async function refresh_backups_info() {
  const result = await commands.getGameSnapshotsInfo(game.value);
  if (result.status === 'error') {
    showError({ message: result.error });
  } else {
    gameSnapshots.value = result.data;
    table_data.value = result.data.backups;
  }
}

async function send_save_to_background() {
  showInfo({ message: $t('manage.wait_for_prompt_hint') });
  if (!backup_button_time_limit) {
    showError({ message: $t('manage.save_too_fast_error') });
    return;
  }
  if (!backup_button_backup_limit) {
    showError({ message: $t('manage.last_backup_unfinished_error') });
    return;
  }
  if (!apply_button_apply_limit) {
    showError({ message: $t('manage.last_overwrite_unfinished_error') });
    return;
  }
  backup_button_time_limit = false;
  backup_button_backup_limit = false;

  await withLoading(async () => {
    const result = await commands.createSnapshot(game.value, describe.value);
    if (result.status === 'error') {
      showError({ message: result.error });
    } else {
      const msg = config.value.settings.cloud_settings?.always_sync
        ? $t('manage.backup_success_with_sync')
        : $t('manage.backup_success');
      showSuccess({ message: msg });
    }
  }, $t('manage.creating_backup'));
  backup_button_backup_limit = true;
  refresh_backups_info();

  describe.value = '';
  setTimeout(() => {
    backup_button_time_limit = true;
  }, 1000);
}

async function create_new_save() {
  if (config.value.settings.prompt_when_not_described && !describe.value) {
    try {
      await feedback.confirm($t('manage.no_description_warning'), $t('manage.warning'), {
        confirmButtonText: $t('manage.confirm_save'),
        cancelButtonText: $t('manage.cancel'),
        type: 'warning',
      });
      send_save_to_background();
    } catch {
      info('User cancelled the save operation.');
    }
  } else {
    send_save_to_background();
  }
}

async function launch_game() {
  // 获取当前设备的游戏路径
  let gamePath = '';
  if (currentDevice.value && game.value.game_paths) {
    gamePath = game.value.game_paths[currentDevice.value.id] || '';
  }

  if (!gamePath) {
    showError({ message: $t('manage.no_launch_path_error') });
    return;
  } else {
    const result = await commands.openFileOrFolder(gamePath);
    if (result.status === 'error') {
      showError({ message: result.error });
    }
  }
}

async function del_save(date: string) {
  try {
    await commands.deleteSnapshot(game.value, date);
    refresh_backups_info();
    showSuccess({ message: $t('manage.delete_success') });
  } catch (e) {
    error(`Failed to delete snapshot: ${e}`);
    showError({ message: $t('error.delete_snapshot_failed') });
  }
}

async function apply_save(date: string) {
  showInfo({ message: $t('manage.wait_for_prompt_hint') });

  if (!apply_button_apply_limit) {
    showError({ message: $t('manage.last_overwrite_unfinished_error') });
    return;
  }
  if (!backup_button_backup_limit) {
    showError({ message: $t('manage.last_backup_unfinished_error') });
    return;
  }
  apply_button_apply_limit = false;

  // 记录应用前的 HEAD，用于撤销
  const previousHead = currentHead.value ?? null;

  await withLoading(async () => {
    const result = await commands.restoreSnapshot(game.value, date);
    if (result.status === 'error') {
      showError({ message: $t('manage.recover_failed') });
    } else {
      showSuccess({ message: $t('manage.recover_success') });

      // 尝试获取刚创建的额外备份，用于撤销
      if (extraBackupEnabled.value) {
        try {
          const extraResult = await commands.getGameExtraBackups(game.value);
          if (extraResult.status === 'ok' && extraResult.data.length > 0) {
            const latestExtra = extraResult.data[0];
            if (latestExtra) {
              undoInfo.value = {
                extraBackupDate: latestExtra.date,
                previousHead,
              };
            }
          }
        } catch (e) {
          error(`Failed to get extra backups for undo: ${e}`);
        }
      }
    }
  }, $t('manage.restoring_backup'));
  apply_button_apply_limit = true;
  refresh_backups_info();
}

async function undo_last_apply() {
  if (!undoInfo.value) return;

  try {
    await feedback.confirm($t('manage.undo_confirm'), $t('manage.warning'), {
      confirmButtonText: $t('manage.confirm'),
      cancelButtonText: $t('manage.cancel'),
      type: 'warning',
    });
  } catch {
    return;
  }

  const { extraBackupDate, previousHead } = undoInfo.value;

  await withLoading(async () => {
    const result = await commands.restoreExtraBackup(game.value, extraBackupDate);
    if (result.status === 'error') {
      showError({ message: $t('manage.undo_failed') });
      return;
    }

    // 恢复之前的 HEAD 指针
    if (previousHead) {
      const headResult = await commands.setSnapshotHead(game.value, previousHead);
      if (headResult.status === 'error') {
        error(`Failed to restore HEAD on undo: ${headResult.error}`);
      }
    }

    undoInfo.value = null;
    showSuccess({ message: $t('manage.undo_success') });
  }, $t('manage.restoring_backup'));

  refresh_backups_info();
}

async function change_describe(date: string) {
  try {
    const { value } = await feedback.prompt(
      $t('manage.input_description_prompt'),
      $t('manage.change_description'),
      {
        confirmButtonText: $t('manage.confirm'),
        cancelButtonText: $t('manage.cancel'),
        inputValue: table_data.value.find((x) => x.date == date)?.describe,
      }
    );
    const result = await commands.setSnapshotDescription(game.value, date, value);
    if (result.status === 'error') {
      // TODO: 增加文本
      showError({ message: $t('manage.change_description_failed') });
    }
    refresh_backups_info();
    showSuccess({ message: $t('manage.change_description_success') });
  } catch {
    showInfo({ message: $t('manage.operation_canceled') });
  }
}

function load_latest_save() {
  // 数组是正序的，最后一个是最新的，而展示用的filter_table是倒序的
  const lastIndex = table_data.value.length - 1;
  const lastBackup = lastIndex >= 0 ? table_data.value[lastIndex] : undefined;

  if (lastBackup?.date) {
    apply_save(lastBackup.date);
  } else {
    showError({ message: $t('manage.no_backup_error') });
  }
}

async function del_cur() {
  try {
    const { value } = await feedback.prompt($t('manage.delete_prompt'), $t('home.hint'), {
      confirmButtonText: $t('manage.confirm'),
      cancelButtonText: $t('manage.cancel'),
      inputPattern: /yes/,
      inputErrorMessage: $t('manage.invalid_input_error'),
    });

    if (value === 'yes') {
      const result = await commands.deleteGame(game.value);
      if (result.status === 'error') {
        showError({ message: $t('error.delete_game_failed') });
      }
      await refreshConfig();
      router.back();
    } else {
      showInfo({ message: $t('manage.invalid_input_error') });
    }
  } catch {
    showInfo({ message: $t('manage.operation_canceled') });
  }
}

async function open_backup_folder() {
  const result = await commands.openBackupFolder(game.value);
  if (result.status === 'error') {
    showError({ message: $t('error.open_backup_folder_failed') });
  }
}

// 设置快速备份，由快捷键和tray触发备份和恢复
async function set_quick_backup() {
  const result = await commands.setQuickBackupGame(game.value);
  if (result.status === 'error') {
    showError({ message: $t('manage.set_quick_backup_failed') });
    return;
  }
  showSuccess({ message: $t('manage.set_quick_backup_success') });
}

// 处理抽屉组件保存游戏路径的事件
async function on_drawer_save_changes(updatedGame: Game) {
  // 更新游戏信息（包括存档路径和启动路径）
  game.value = updatedGame;

  // 保存到配置
  const index = config.value.games.findIndex((g) => g.name === game.value.name);
  if (index !== -1) {
    config.value.games[index] = game.value;
    saveConfig()
      .then(() => {
        showSuccess({ message: $t('manage.save_paths_updated') });
      })
      .catch((e) => {
        error(`Error saving config: ${e}`);
        showError({ message: $t('error.save_config_failed') });
      });
  } else {
    showError({ message: $t('error.game_not_found') });
  }

  // 关闭侧栏
  drawer.value = false;
}

const filter_table = computed(() => {
  return table_data.value
    .filter(
      (data) =>
        !search.value || data.describe.includes(search.value) || data.date.includes(search.value)
    )
    .reverse();
});

// 检查当前设备的存档路径是否为空
async function checkCurrentDeviceSavePaths() {
  await fetchCurrentDevice();
  if (!currentDevice.value || !game.value || !game.value.save_paths) return;

  // 检查当前设备的存档路径是否全部为空
  const deviceId = currentDevice.value.id;
  const allPathsEmpty = game.value.save_paths.every(
    (unit) => !unit.paths || !unit.paths[deviceId] || unit.paths[deviceId].trim() === ''
  );

  if (!allPathsEmpty) return; // 如果有路径不为空，直接返回

  // 收集所有有效的设备ID（有存档路径的设备）
  const devicesWithPaths = new Set<string>();
  game.value.save_paths.forEach((unit) => {
    if (unit.paths) {
      Object.entries(unit.paths).forEach(([id, path]) => {
        if (id !== deviceId && path && path.trim() !== '') {
          devicesWithPaths.add(id);
        }
      });
    }
  });

  if (devicesWithPaths.size === 0) return; // 如果没有其他设备有路径，直接返回

  try {
    // 询问用户是否要复制其他设备的存档路径
    const confirmResult = await feedback.confirm(
      $t('manage.empty_paths_prompt'),
      $t('manage.empty_paths_title'),
      {
        confirmButtonText: $t('manage.copy_from_device'),
        cancelButtonText: $t('manage.keep_empty'),
        type: 'info',
        closeOnClickModal: false,
        closeOnPressEscape: false,
      }
    );

    if (confirmResult !== 'confirm') return;

    // 准备设备选择列表
    const deviceOptions = Array.from(devicesWithPaths).map((id) => ({
      value: id,
      label: id.substring(0, 8) + '...',
    }));

    // 如果只有一个设备，直接使用它
    if (deviceOptions.length === 1) {
      const singleOption = deviceOptions.at(0);
      if (singleOption) {
        await copyPathsFromDevice(singleOption.value);
        return;
      }
    }

    // 让用户从多个设备中选择
    try {
      // 显示设备列表供用户选择
      const items = deviceOptions
        .map((d, index) => `${index + 1}. ${d.label} (${d.value})`)
        .join('\n');

      const { value } = await feedback.prompt(
        `${$t('manage.select_device_prompt')}\n\n${items}\n\n${$t('manage.enter_device_id')}:`,
        $t('manage.select_device_title'),
        {
          confirmButtonText: $t('manage.confirm'),
          cancelButtonText: $t('manage.cancel'),
        }
      );

      // 查找匹配的设备ID
      const selectedDevice = deviceOptions.find(
        (d) => d.value === value || d.value.startsWith(value) || d.label.includes(value)
      );

      if (selectedDevice) {
        await copyPathsFromDevice(selectedDevice.value);
      }
    } catch {
      // 用户取消选择，不执行任何操作
    }
  } catch {
    // 用户取消初始确认，不执行任何操作
  }
}

// 从指定设备复制存档路径到当前设备
async function copyPathsFromDevice(sourceDeviceId: string) {
  if (!currentDevice.value || !game.value) return;

  const targetDeviceId = currentDevice.value.id;
  let updated = false;

  // 复制存档路径
  if (game.value.save_paths) {
    game.value.save_paths.forEach((unit) => {
      if (unit.paths?.[sourceDeviceId]?.trim()) {
        if (!unit.paths[targetDeviceId] || !unit.paths[targetDeviceId].trim()) {
          if (!unit.paths) unit.paths = {};
          unit.paths[targetDeviceId] = unit.paths[sourceDeviceId];
          updated = true;
        }
      }
    });
  }

  // 复制游戏启动路径
  if (
    game.value.game_paths?.[sourceDeviceId]?.trim() &&
    (!game.value.game_paths[targetDeviceId] || !game.value.game_paths[targetDeviceId].trim())
  ) {
    if (!game.value.game_paths) game.value.game_paths = {};
    game.value.game_paths[targetDeviceId] = game.value.game_paths[sourceDeviceId];
    updated = true;
  }

  // 如果有更新，保存配置
  if (updated) {
    const index = config.value.games.findIndex((g) => g.name === game.value.name);
    if (index !== -1) {
      config.value.games[index] = game.value;
      try {
        await saveConfig();
        showSuccess({ message: $t('manage.paths_copied_success') });
        // 打开侧栏让用户查看和编辑复制的路径
        drawer.value = true;
      } catch (e) {
        error(`Error saving config: ${e}`);
        showError({ message: $t('error.save_config_failed') });
      }
    }
  }
}

// Branch tree view handlers
async function onSetHead(date: string) {
  try {
    const result = await commands.setSnapshotHead(game.value, date);
    if (result.status === 'error') {
      showError({ message: $t('manage.set_head_failed') });
    } else {
      showSuccess({ message: $t('manage.set_head_success') });
      await refresh_backups_info();
    }
  } catch (e) {
    error(`Failed to set HEAD: ${e}`);
    showError({ message: $t('manage.set_head_failed') });
  }
}

async function onDetach(date: string) {
  try {
    const result = await commands.detachSnapshot(game.value, date);
    if (result.status === 'error') {
      showError({ message: $t('manage.detach_failed') });
    } else {
      showSuccess({ message: $t('manage.detach_success') });
      await refresh_backups_info();
    }
  } catch (e) {
    error(`Failed to detach snapshot: ${e}`);
    showError({ message: $t('manage.detach_failed') });
  }
}

async function onCreateBranch(parentDate: string) {
  try {
    const { value } = await feedback.prompt(
      $t('manage.input_description_prompt'),
      $t('manage.create_branch'),
      {
        confirmButtonText: $t('manage.confirm'),
        cancelButtonText: $t('manage.cancel'),
      }
    );

    await withLoading(async () => {
      const result = await commands.createSnapshotAt(game.value, value || '', parentDate);
      if (result.status === 'error') {
        showError({ message: result.error });
      } else {
        const msg = config.value.settings.cloud_settings?.always_sync
          ? $t('manage.backup_success_with_sync')
          : $t('manage.backup_success');
        showSuccess({ message: msg });
        await refresh_backups_info();
      }
    }, $t('manage.creating_backup'));
  } catch {
    // User cancelled
  }
}

// Computed property for current HEAD
const currentHead = computed(() => gameSnapshots.value?.head ?? null);

// Format HEAD for human-readable display
const currentHeadSnapshot = computed(() => {
  if (!currentHead.value) return null;
  return table_data.value.find((s) => s.date === currentHead.value) ?? null;
});

const currentHeadDescription = computed(() => {
  const snapshot = currentHeadSnapshot.value;
  if (!snapshot) return '';
  return snapshot.describe?.trim() || '';
});

const currentHeadTime = computed(() => {
  const snapshot = currentHeadSnapshot.value;
  if (!snapshot) return '';
  const parsed = dayjs(snapshot.date, 'YYYY-MM-DD_HH-mm-ss');
  return parsed.isValid() ? parsed.format('MM/DD HH:mm') : snapshot.date;
});

const currentHeadFullText = computed(() => {
  const desc = currentHeadDescription.value;
  const time = currentHeadTime.value;
  if (desc) {
    return `${desc} (${time})`;
  }
  return time;
});
</script>

<template>
  <div class="manage-container">
    <!-- Page Header -->
    <div class="page-header">
      <h2 class="page-title">{{ game.name }}</h2>
      <div class="header-actions">
        <el-tooltip :content="$t('manage.launch_game')" placement="bottom">
          <el-button circle :icon="VideoPlay" type="success" @click="launch_game" />
        </el-tooltip>
        <el-tooltip :content="$t('manage.open_backup_folder')" placement="bottom">
          <el-button circle :icon="Folder" @click="open_backup_folder" />
        </el-tooltip>
        <el-tooltip :content="$t('manage.extra_backups')" placement="bottom">
          <el-button circle :icon="DocumentCopy" @click="extraBackupDrawer = true" />
        </el-tooltip>
        <el-tooltip :content="$t('manage.show_drawer')" placement="bottom">
          <el-button circle :icon="Setting" @click="drawer = true" />
        </el-tooltip>
        <el-tooltip :content="$t('manage.set_quick_backup')" placement="bottom">
          <el-button circle :icon="Timer" type="warning" @click="set_quick_backup" />
        </el-tooltip>
        <el-tooltip :content="$t('manage.delete_save_manage')" placement="bottom">
          <el-button circle :icon="Delete" type="danger" @click="del_cur" />
        </el-tooltip>
      </div>
    </div>

    <!-- Quick Actions Card -->
    <el-card class="quick-actions-card" shadow="never">
      <div class="quick-actions-content">
        <div class="create-backup-section">
          <el-input
            v-model="describe"
            :placeholder="$t('manage.input_description_prompt')"
            class="backup-input"
            @keyup.enter="create_new_save"
          >
            <template #append>
              <el-button type="primary" :icon="Plus" @click="create_new_save">
                {{ $t('manage.create_new_save') }}
              </el-button>
            </template>
          </el-input>
        </div>
        <el-divider direction="vertical" class="action-divider" />
        <div class="restore-section">
          <el-button type="warning" :icon="VideoPlay" @click="load_latest_save">
            {{ $t('manage.load_latest_save') }}
          </el-button>
          <el-tooltip :content="undoTooltip" placement="bottom">
            <span>
              <el-button
                circle
                :type="canUndo && extraBackupEnabled ? 'success' : ''"
                :icon="Back"
                :disabled="!canUndo || !extraBackupEnabled"
                @click="undo_last_apply"
              />
            </span>
          </el-tooltip>
        </div>
      </div>
    </el-card>

    <!-- Main Content Area -->
    <el-card class="main-content-card" shadow="never">
      <template #header>
        <div class="content-header">
          <div class="left-controls">
            <el-radio-group v-model="viewMode" size="small">
              <el-radio-button value="table">
                <el-icon class="mr-1"><List /></el-icon>
                {{ $t('manage.table_view') }}
              </el-radio-button>
              <el-radio-button value="branch">
                <el-icon class="mr-1"><Share /></el-icon>
                {{ $t('manage.branch_view') }}
              </el-radio-button>
            </el-radio-group>

            <el-divider direction="vertical" />

            <el-input
              v-if="viewMode === 'table'"
              v-model="search"
              size="small"
              :placeholder="$t('manage.input_description_search_prompt')"
              clearable
              style="width: 200px"
            />

            <el-button
              v-if="selected_game_snapshots.length > 0 && viewMode === 'table'"
              type="danger"
              size="small"
              plain
              :icon="Delete"
              @click="batch_delete()"
            >
              {{ $t('manage.batch_delete') }}
            </el-button>
          </div>

          <el-tooltip
            v-if="currentHead"
            :content="currentHeadFullText"
            placement="bottom-end"
            :show-after="300"
            popper-class="head-tooltip"
          >
            <el-tag type="success" effect="plain" round class="head-tag">
              <span class="head-label">
                {{ $t('manage.current_position') }}
              </span>
              <span class="head-separator">·</span>
              <span v-if="currentHeadDescription" class="head-desc">
                {{ currentHeadDescription }}
              </span>
              <span class="head-time">{{ currentHeadTime }}</span>
            </el-tag>
          </el-tooltip>
        </div>
      </template>

      <!-- Table View -->
      <div v-if="viewMode === 'table'" class="view-container">
        <el-empty v-if="filter_table.length === 0" :description="$t('manage.no_snapshots')" />
        <el-table v-else :data="filter_table" height="100%" @selection-change="on_selection_change">
          <el-table-column type="selection" width="40" />
          <el-table-column :label="$t('manage.save_date')" prop="date" width="180" sortable>
            <template #default="{ row }">
              <span class="font-mono text-sm">{{ row.date }}</span>
            </template>
          </el-table-column>
          <el-table-column
            :label="$t('manage.description')"
            prop="describe"
            min-width="200"
            show-overflow-tooltip
          />
          <el-table-column :label="$t('manage.size')" width="100">
            <template #default="{ row }">
              <span class="text-gray-500 text-xs">{{
                row.size ? formatFileSize(row.size) : '-'
              }}</span>
            </template>
          </el-table-column>
          <el-table-column :label="$t('manage.actions')" align="center" width="140" fixed="right">
            <template #default="{ row }">
              <div class="action-buttons">
                <el-tooltip
                  :content="$t('manage.apply')"
                  placement="top"
                  :show-after="300"
                  popper-class="action-tooltip"
                >
                  <span>
                    <el-popconfirm
                      :title="$t('manage.confirm_overwrite_prompt')"
                      @confirm="apply_save(row.date)"
                    >
                      <template #reference>
                        <el-button link type="success" :icon="VideoPlay" />
                      </template>
                    </el-popconfirm>
                  </span>
                </el-tooltip>
                <el-tooltip
                  :content="$t('manage.change_describe')"
                  placement="top"
                  :show-after="300"
                  popper-class="action-tooltip"
                >
                  <el-button link type="warning" :icon="Edit" @click="change_describe(row.date)" />
                </el-tooltip>
                <el-tooltip
                  :content="$t('manage.delete')"
                  placement="top"
                  :show-after="300"
                  popper-class="action-tooltip"
                >
                  <span>
                    <el-popconfirm
                      :title="$t('manage.confirm_delete_prompt')"
                      @confirm="del_save(row.date)"
                    >
                      <template #reference>
                        <el-button link type="danger" :icon="Delete" />
                      </template>
                    </el-popconfirm>
                  </span>
                </el-tooltip>
              </div>
            </template>
          </el-table-column>
        </el-table>
      </div>

      <!-- Branch View -->
      <div v-else ref="branchViewContainer" class="view-container branch-view">
        <BranchTreeView
          v-if="viewMode === 'branch'"
          :snapshots="table_data"
          :head="currentHead"
          @apply="apply_save"
          @delete="del_save"
          @change-description="change_describe"
          @set-head="onSetHead"
          @detach="onDetach"
          @create-branch="onCreateBranch"
        />
      </div>
    </el-card>

    <!-- Drawer -->
    <save-location-drawer
      v-if="game"
      v-model="drawer"
      :game="game"
      @closed="drawer = false"
      @save-changes="on_drawer_save_changes"
    />

    <ExtraBackupDrawer v-if="game" v-model="extraBackupDrawer" :game="game" />
  </div>
</template>

<style scoped>
.manage-container {
  /* ElMain has default 20px padding, so we subtract 40px from 100vh to fit exactly */
  height: calc(100vh - 40px);
  display: flex;
  flex-direction: column;
  gap: 16px;
  box-sizing: border-box;
  overflow: hidden;
}

.page-header {
  flex-shrink: 0;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  min-width: 0;
}

.page-title {
  margin: 0;
  font-size: 20px;
  font-weight: 600;
  color: var(--el-text-color-primary);
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.header-actions {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-shrink: 0;
}

.quick-actions-card {
  flex-shrink: 0;
  border-radius: 8px;
  overflow: visible;
}

.quick-actions-content {
  display: flex;
  align-items: center;
  gap: 16px;
}

.create-backup-section {
  flex: 1;
}

.backup-input :deep(.el-input-group__append) {
  background-color: var(--el-color-primary);
  border-color: var(--el-color-primary);
  color: white;
}

.backup-input :deep(.el-input-group__append button:hover) {
  color: white;
  opacity: 0.9;
}

.action-divider {
  height: 24px;
}

.restore-section {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-shrink: 0;
}

.main-content-card {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  border-radius: 8px;
  overflow: hidden;
}

.main-content-card :deep(.el-card__header) {
  flex-shrink: 0;
  padding: 12px 16px;
}

.main-content-card :deep(.el-card__body) {
  flex: 1;
  min-height: 0;
  padding: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.content-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.left-controls {
  display: flex;
  align-items: center;
  gap: 12px;
}

.view-container {
  flex: 1;
  min-height: 0;
  overflow: auto;
  width: 100%;
  height: 100%;
}

.branch-view {
  background-color: #f5f7fa;
  overflow: hidden;
}

.text-danger {
  color: var(--el-color-danger);
}

.font-mono {
  font-family: var(--el-font-family-monospace);
}

.mr-1 {
  margin-right: 4px;
}

.head-tag {
  max-width: 260px;
  display: inline-flex;
  align-items: center;
  cursor: default;
  padding: 0 10px;
  box-sizing: border-box;
}

.head-label {
  flex-shrink: 0;
  color: var(--el-color-success-dark-2);
  font-weight: 500;
}

.head-separator {
  flex-shrink: 0;
  margin: 0 6px;
  color: var(--el-color-success-light-3);
}

.head-desc {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 120px;
  color: var(--el-text-color-primary);
  font-weight: 500;
  margin-right: 8px;
}

.head-time {
  flex-shrink: 0;
  color: var(--el-text-color-secondary);
  font-size: 12px;
  opacity: 0.85;
}

.action-buttons {
  display: flex;
  justify-content: center;
  gap: 8px;
}

.action-buttons .el-button {
  font-size: 16px;
}

:deep(.head-tooltip),
:deep(.action-tooltip) {
  max-width: 260px;
  white-space: normal;
  word-break: break-word;
}
</style>
