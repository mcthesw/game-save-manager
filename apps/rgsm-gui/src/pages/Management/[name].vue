<script lang="ts" setup>
import { computed, ref, watch, onBeforeUnmount, onMounted, h } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { commands, events } from '../../bindings';
import SaveLocationDrawer from '../../components/SaveLocationDrawer.vue';
import AutoSaveSettingsDrawer from '../../components/AutoSaveSettingsDrawer.vue';
import BranchTreeView from '../../components/BranchTreeView.vue';
import ExtraBackupDrawer from '../../components/ExtraBackupDrawer.vue';
import type { CandidateDimensions, Game, GameSnapshots, Snapshot, Device } from '../../bindings';
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
  Lightning,
  AlarmClock,
  Edit,
  CircleCheck,
  Lock,
} from '@element-plus/icons-vue';
import dayjs from 'dayjs';
import {
  getGameManagementPath,
  getGameNameFromRouteParam,
} from '../../composables/useGameManagementRoute';
import { useApplyConfirmation } from '../../composables/useApplyConfirmation';
import { usePathResolution } from '../../composables/usePathResolution';
import { TableV2FixedDir, TableV2SortOrder } from '../../ui/elementPlus/tableV2';
import { saveUnitPaths } from '../../utils/saveUnit';

const { addActivity, updateActivity } = useActivityCenter();
const feedback = useFeedback();
const { config, refreshConfig, saveConfig } = useConfig();
const { confirmAndRun } = useApplyConfirmation();
const { markGamePlayed } = useSaveListSort();
const { withLoading } = useGlobalLoading();
const { startCollecting, stopCollecting } = useIpcNotificationCollector();
const { preview: previewSaveUnit, rememberRestoreMapping } = usePathResolution();
const router = useRouter();
const route = useRoute();

// View mode: 'table' or 'branch'
const viewMode = ref<'table' | 'branch'>('table');

const search = ref(''); // 搜索时使用的字符串
const drawer = ref(false); // 是否显示存档位置侧栏
const extraBackupDrawer = ref(false); // 是否显示额外备份抽屉
const autoSaveSettingsDrawer = ref(false); // 是否显示自动保存设置抽屉

const table_data = ref<Snapshot[]>([]);
const table_data_desc = ref<Snapshot[]>([]);
const tableSortBy = ref<{ key: string; order: TableV2SortOrder }>({
  key: 'date',
  order: TableV2SortOrder.DESC,
});
const selectedDates = ref<Set<string>>(new Set());

// Game snapshots info including HEAD
const gameSnapshots = ref<GameSnapshots | null>(null);

const game: Ref<Game> = ref({
  name: '',
  storage_key: '',
  save_paths: [],
  game_paths: {},
  device_bindings: {},
});

// 当前设备信息
const currentDevice = ref<Device | null>(null);

type GameSnapshotsWithDeviceHeads = GameSnapshots & {
  device_heads?: Record<string, string | null | undefined>;
};

interface DeviceHeadEntry {
  deviceId: string;
  deviceName: string;
  label: string;
  date: string;
  description: string;
  shortTime: string;
  fullText: string;
  isCurrentDevice: boolean;
}

interface BranchDeviceHeadMarker {
  deviceId: string;
  date: string;
  label: string;
  isCurrentDevice: boolean;
  tooltip: string;
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
  if (canUndo.value) return $t('manage.undo_last_apply');
  if (!extraBackupEnabled.value) return $t('manage.undo_requires_extra_backup');
  return $t('manage.undo_not_available');
});

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

function snapshotSourceTag(snapshot: Snapshot): string | null {
  if (snapshot.created_by === 'Timer') return $t('manage.snapshot_source_timer');
  if (snapshot.created_by === 'Tray') return $t('manage.snapshot_source_tray');
  if (snapshot.created_by === 'Hotkey') return $t('manage.snapshot_source_hotkey');
  if (snapshot.created_by === 'ProcessStart') return $t('manage.snapshot_source_process_start');
  if (snapshot.created_by === 'ProcessExit') return $t('manage.snapshot_source_process_exit');
  if (snapshot.created_by === 'ProcessInterval')
    return $t('manage.snapshot_source_process_interval');
  return null;
}

function isAutomaticSnapshot(snapshot: Snapshot): boolean {
  return (
    snapshot.created_by === 'Timer' ||
    snapshot.created_by === 'ProcessStart' ||
    snapshot.created_by === 'ProcessExit' ||
    snapshot.created_by === 'ProcessInterval'
  );
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
        notifySuccess($t('manage.batch_delete_success', { count: dates.length }));
      } else {
        notifyError(deleteResult.error);
      }
    } else {
      notifyInfo($t('manage.invalid_input_error'));
    }
  } catch {
    notifyError($t('manage.operation_canceled'));
  }
}

const autoSaveConfigured = computed(() => isAutoSaveConfigured(config.value, game.value));

async function onAutoSaveSettingsSaved() {
  await refreshConfig();
  const latestGame = config.value.games.find((item) => item.name === game.value.name);
  if (latestGame) {
    game.value = latestGame;
  }
  await refresh_backups_info();
}

async function convertToPermanent(snapshotDate: string) {
  try {
    const snapshot = table_data.value.find((x) => x.date === snapshotDate);
    const { value } = await feedback.prompt(
      $t('manage.input_description_prompt'),
      $t('manage.convert_to_permanent'),
      {
        confirmButtonText: $t('manage.confirm'),
        cancelButtonText: $t('manage.cancel'),
        inputValue: snapshot?.describe,
      }
    );
    if (value !== snapshot?.describe) {
      const descResult = await commands.setSnapshotDescription(game.value, snapshotDate, value);
      if (descResult.status === 'error') {
        notifyError($t('manage.change_description_failed'));
        return;
      }
    }
    const result = await commands.setSnapshotCreatedBy(game.value.name, snapshotDate, 'Manual');
    if (result.status === 'error') {
      notifyError($t('manage.convert_to_permanent_failed'));
      return;
    }
    notifySuccess($t('manage.convert_to_permanent_success'));
    await refresh_backups_info();
  } catch {
    notifyInfo($t('manage.operation_canceled'));
  }
}

// Init game info
watch(
  () => ('name' in route.params ? route.params.name : undefined),
  (newValue) => {
    if (!newValue) {
      return;
    }
    const name = getGameNameFromRouteParam(newValue);
    game.value = config.value.games.find((x) => x.name == name) as Game;
    undoInfo.value = null;
    refresh_backups_info();
    // 检查当前设备的存档路径是否为空
    checkCurrentDeviceSavePaths();
  },
  { immediate: true }
);

async function refresh_backups_info() {
  const result = await commands.getGameSnapshotsInfo(game.value);
  if (result.status === 'error') {
    notifyError(result.error);
  } else {
    gameSnapshots.value = result.data;
    table_data.value = result.data.backups;
    table_data_desc.value = [...result.data.backups].reverse();
    selectedDates.value = new Set();
  }
}

function backupSuccessMessage() {
  const backendEnabled = config.value?.settings.cloud_settings?.backend?.type !== 'Disabled';
  return backendEnabled && game.value.cloud_sync_enabled !== false
    ? $t('manage.backup_success_with_sync')
    : $t('manage.backup_success');
}

function formatSnapshotPromptLine(date: string) {
  const snapshot = table_data.value.find((item) => item.date === date);
  const parsed = dayjs(date, 'YYYY-MM-DD_HH-mm-ss');
  const formatted = parsed.isValid() ? parsed.format('YYYY-MM-DD HH:mm:ss') : date;
  const description = snapshot?.describe?.trim();
  return description ? `${formatted} · ${description}` : formatted;
}

function findHeadEntryByInput(entries: DeviceHeadEntry[], rawInput: string) {
  const input = rawInput.trim();
  if (!input) return null;

  const index = Number.parseInt(input, 10);
  if (Number.isInteger(index) && index >= 1 && index <= entries.length) {
    return entries[index - 1] ?? null;
  }

  return (
    entries.find(
      (entry) =>
        entry.deviceId === input ||
        entry.deviceId.startsWith(input) ||
        entry.deviceName.toLowerCase().includes(input.toLowerCase())
    ) ?? null
  );
}

function findSnapshotByInput(rawInput: string) {
  const input = rawInput.trim();
  if (!input) return null;

  const recentSnapshots = [...table_data.value].slice(-10).reverse();
  const index = Number.parseInt(input, 10);
  if (Number.isInteger(index) && index >= 1 && index <= recentSnapshots.length) {
    return recentSnapshots[index - 1] ?? null;
  }

  return (
    table_data.value.find(
      (snapshot) =>
        snapshot.date === input ||
        snapshot.date.startsWith(input) ||
        snapshot.describe.toLowerCase().includes(input.toLowerCase())
    ) ?? null
  );
}

async function resolveParentForNewSnapshot(): Promise<string | null | undefined> {
  if (!currentDevice.value) {
    await fetchCurrentDevice();
  }

  if (currentHead.value) {
    return currentHead.value;
  }

  if (!currentDevice.value?.id) {
    return null;
  }

  const otherHeads = headEntries.value.filter((entry) => !entry.isCurrentDevice);
  if (otherHeads.length === 0) {
    return null;
  }

  try {
    const { value } = await feedback.prompt(
      [
        $t('manage.first_snapshot_prompt'),
        '',
        `1. ${$t('manage.first_snapshot_use_device_head')}`,
        `2. ${$t('manage.first_snapshot_use_specific_snapshot')}`,
        `3. ${$t('manage.first_snapshot_start_new_tree')}`,
      ].join('\n'),
      $t('manage.first_snapshot_title'),
      {
        confirmButtonText: $t('manage.confirm'),
        cancelButtonText: $t('manage.cancel'),
        inputPattern: /^[123]$/,
        inputErrorMessage: $t('manage.first_snapshot_invalid_choice'),
      }
    );

    if (value === '1') {
      if (otherHeads.length === 1) {
        return otherHeads[0]?.date ?? null;
      }

      const choices = otherHeads
        .map((entry, index) => `${index + 1}. ${entry.deviceName} · ${entry.shortTime}`)
        .join('\n');
      const { value: headValue } = await feedback.prompt(
        `${$t('manage.select_head_device_prompt')}

${choices}`,
        $t('manage.select_device_title'),
        {
          confirmButtonText: $t('manage.confirm'),
          cancelButtonText: $t('manage.cancel'),
        }
      );

      const matchedHead = findHeadEntryByInput(otherHeads, headValue);
      if (!matchedHead) {
        notifyError($t('manage.invalid_snapshot_or_device'));
        return undefined;
      }
      return matchedHead.date;
    }

    if (value === '2') {
      const recentSnapshots = [...table_data.value].slice(-10).reverse();
      const items = recentSnapshots
        .map((snapshot, index) => `${index + 1}. ${formatSnapshotPromptLine(snapshot.date)}`)
        .join('\n');
      const { value: snapshotValue } = await feedback.prompt(
        `${$t('manage.select_snapshot_prompt')}

${items}`,
        $t('manage.select_snapshot_title'),
        {
          confirmButtonText: $t('manage.confirm'),
          cancelButtonText: $t('manage.cancel'),
        }
      );

      const matchedSnapshot = findSnapshotByInput(snapshotValue);
      if (!matchedSnapshot) {
        notifyError($t('manage.invalid_snapshot_or_device'));
        return undefined;
      }
      return matchedSnapshot.date;
    }

    return null;
  } catch {
    return undefined;
  }
}

async function send_save_to_background() {
  if (!backup_button_time_limit) {
    notifyError($t('manage.save_too_fast_error'));
    return;
  }
  if (!backup_button_backup_limit) {
    notifyError($t('manage.last_backup_unfinished_error'));
    return;
  }
  if (!apply_button_apply_limit) {
    notifyError($t('manage.last_overwrite_unfinished_error'));
    return;
  }

  const parentDate = await resolveParentForNewSnapshot();
  if (parentDate === undefined) {
    return;
  }

  backup_button_time_limit = false;
  backup_button_backup_limit = false;

  const activityId = addActivity({ title: $t('manage.creating_backup'), status: 'running' });
  try {
    await withLoading(
      async () => {
        const result = await commands.createSnapshotAt(game.value, describe.value, parentDate);
        if (result.status === 'error') {
          updateActivity(activityId, { status: 'error', description: result.error });
        } else {
          updateActivity(activityId, { status: 'success', title: backupSuccessMessage() });
        }
      },
      $t('manage.creating_backup'),
      $t('manage.wait_for_prompt_hint')
    );
  } catch {
    updateActivity(activityId, { status: 'error' });
  }
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
    notifyError($t('manage.no_launch_path_error'));
    return;
  } else {
    const result = await commands.openFileOrFolder(gamePath);
    if (result.status === 'error') {
      notifyError(result.error);
    } else {
      markGamePlayed(game.value);
    }
  }
}

async function del_save(date: string) {
  try {
    await commands.deleteSnapshot(game.value, date);
    refresh_backups_info();
    notifySuccess($t('manage.delete_success'));
  } catch (e) {
    error(`Failed to delete snapshot: ${e}`);
    notifyError($t('error.delete_snapshot_failed'));
  }
}

async function handleApplyClick(date: string) {
  await confirmAndRun('snapshot', () => apply_save(date));
}

async function apply_save(date: string) {
  if (!apply_button_apply_limit) {
    notifyError($t('manage.last_overwrite_unfinished_error'));
    return;
  }
  if (!backup_button_backup_limit) {
    notifyError($t('manage.last_backup_unfinished_error'));
    return;
  }
  apply_button_apply_limit = false;

  // 记录应用前的 HEAD，用于撤销
  const previousHead = currentHead.value ?? null;

  // 记录应用前最新额外备份的 date，用于验证新备份是否成功创建
  let latestExtraDateBefore: string | null = null;
  if (extraBackupEnabled.value) {
    try {
      const beforeResult = await commands.getGameExtraBackups(game.value);
      if (beforeResult.status === 'ok' && beforeResult.data.length > 0 && beforeResult.data[0]) {
        latestExtraDateBefore = beforeResult.data[0].date;
      }
    } catch {
      // ignore
    }
  }

  let integrityFailed = false;
  let restoreError = '';
  let mappingError: {
    saveUnitId: number;
    sourceDimensions: CandidateDimensions;
  } | null = null;

  const activityId = addActivity({ title: $t('manage.restoring_backup'), status: 'running' });
  startCollecting();
  try {
    await withLoading(
      async () => {
        const result = await commands.restoreSnapshot(game.value, date);
        if (result.status === 'error') {
          const err = result.error;
          if (err.type === 'IntegrityCheckFailed') {
            integrityFailed = true;
          } else if (err.type === 'BackupNotFound') {
            restoreError = $t('manage.backup_not_found', { date: err.date });
          } else if (err.type === 'RestoreMappingRequired' || err.type === 'StaleRestoreMapping') {
            mappingError = {
              saveUnitId: err.save_unit_id,
              sourceDimensions: err.source_dimensions,
            };
          } else {
            restoreError = err.message;
          }
        } else {
          // 验证最新额外备份已更新（date 不同），才启用撤销
          if (extraBackupEnabled.value) {
            try {
              const extraResult = await commands.getGameExtraBackups(game.value);
              if (extraResult.status === 'ok' && extraResult.data.length > 0) {
                const latestExtra = extraResult.data[0];
                if (latestExtra && latestExtra.date !== latestExtraDateBefore) {
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
      },
      $t('manage.restoring_backup'),
      $t('manage.wait_for_prompt_hint')
    );
  } catch {
    stopCollecting();
    updateActivity(activityId, { status: 'error' });
    apply_button_apply_limit = true;
    refresh_backups_info();
    return;
  }
  const collectedNotifications = stopCollecting();
  apply_button_apply_limit = true;
  refresh_backups_info();

  // Show error dialogs after loading overlay is dismissed
  if (integrityFailed) {
    updateActivity(activityId, { status: 'error', title: $t('manage.integrity_failed_title') });
    try {
      await feedback.alert(
        $t('manage.integrity_failed_detail'),
        $t('manage.integrity_failed_title'),
        { type: 'error', confirmButtonText: $t('manage.confirm') }
      );
    } catch {
      // dialog dismissed
    }
  } else if (mappingError) {
    updateActivity(activityId, { status: 'error', title: $t('manage.choose_restore_location') });
    const mapped = await chooseRestoreLocation(mappingError);
    if (mapped) {
      await apply_save(date);
    }
  } else if (restoreError) {
    updateActivity(activityId, {
      status: 'error',
      title: $t('manage.recover_failed'),
      description: restoreError,
    });
  } else {
    // Consolidate success + any backend warnings into a single activity entry
    const warnings = collectedNotifications.filter((n) => n.level === 'warning');
    if (warnings.length > 0) {
      updateActivity(activityId, {
        status: 'success',
        title: $t('manage.recover_success_with_warnings', { count: warnings.length }),
        description: undefined,
        autoDismissMs: 5000,
      });
    } else {
      updateActivity(activityId, { status: 'success', title: $t('manage.recover_success') });
    }
  }
}

async function chooseRestoreLocation(mapping: {
  saveUnitId: number;
  sourceDimensions: CandidateDimensions;
}): Promise<boolean> {
  const unit = game.value.save_paths.find((candidate) => candidate.id === mapping.saveUnitId);
  if (!unit) return false;
  const preview = await previewSaveUnit(game.value, unit);
  if (!preview || preview.candidates.length === 0) {
    notifyError($t('manage.no_restore_locations'));
    return false;
  }
  const choices = preview.candidates
    .map((candidate, index) => `${index + 1}. ${candidate.expression}`)
    .join('\n');
  try {
    const { value } = await feedback.prompt(
      `${$t('manage.choose_restore_location_hint')}\n\n${choices}`,
      $t('manage.choose_restore_location'),
      { inputPlaceholder: '1' }
    );
    const index = Number(value) - 1;
    const selected = preview.candidates[index];
    if (!selected) {
      notifyWarning($t('manage.invalid_restore_location'));
      return false;
    }
    const result = await rememberRestoreMapping(
      game.value,
      mapping.saveUnitId,
      mapping.sourceDimensions,
      [selected.id]
    );
    if (result.status === 'error') {
      notifyError(result.error);
      return false;
    }
    await refreshConfig();
    const refreshedGame = config.value.games.find((item) => item.name === game.value.name);
    if (refreshedGame) {
      game.value = refreshedGame;
    }
    return true;
  } catch {
    return false;
  }
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

  const activityId = addActivity({ title: $t('manage.restoring_backup'), status: 'running' });
  try {
    await withLoading(async () => {
      const result = await commands.restoreExtraBackup(game.value, extraBackupDate);
      if (result.status === 'error') {
        updateActivity(activityId, { status: 'error', description: $t('manage.undo_failed') });
        return;
      }

      // 恢复之前的 HEAD 指针
      // TODO: 当 previousHead 为 null 时（首次应用前 HEAD 未设置），
      // 需要后端支持 clearSnapshotHead 命令才能完全恢复状态
      if (previousHead) {
        const headResult = await commands.setSnapshotHead(game.value, previousHead);
        if (headResult.status === 'error') {
          error(`Failed to restore HEAD on undo: ${headResult.error}`);
        }
      }

      undoInfo.value = null;
      updateActivity(activityId, { status: 'success', title: $t('manage.undo_success') });
    }, $t('manage.restoring_backup'));
  } catch {
    updateActivity(activityId, { status: 'error' });
  }

  refresh_backups_info();
}

async function change_describe(date: string) {
  try {
    const snapshot = table_data.value.find((x) => x.date == date);
    const { value } = await feedback.prompt(
      $t('manage.input_description_prompt'),
      $t('manage.change_description'),
      {
        confirmButtonText: $t('manage.confirm'),
        cancelButtonText: $t('manage.cancel'),
        inputValue: snapshot?.describe,
      }
    );
    const result = await commands.setSnapshotDescription(game.value, date, value);
    if (result.status === 'error') {
      notifyError($t('manage.change_description_failed'));
      return;
    }
    refresh_backups_info();
    notifySuccess($t('manage.change_description_success'));
  } catch {
    notifyInfo($t('manage.operation_canceled'));
  }
}

async function load_latest_save() {
  // 数组是正序的，最后一个是最新的，而展示用的filter_table是倒序的
  const lastIndex = table_data.value.length - 1;
  const lastBackup = lastIndex >= 0 ? table_data.value[lastIndex] : undefined;

  if (lastBackup?.date) {
    await confirmAndRun('latest', () => apply_save(lastBackup.date));
  } else {
    notifyError($t('manage.no_backup_error'));
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
        notifyError($t('error.delete_game_failed'));
        return;
      }
      await refreshConfig();
      router.back();
    } else {
      notifyInfo($t('manage.invalid_input_error'));
    }
  } catch {
    notifyInfo($t('manage.operation_canceled'));
  }
}

async function open_backup_folder() {
  const result = await commands.openBackupFolder(game.value);
  if (result.status === 'error') {
    notifyError($t('error.open_backup_folder_failed'));
  }
}

async function verify_archive_hashes() {
  const snapshots = table_data.value.filter((s) => s.archive_hash);
  if (snapshots.length === 0) {
    notifyInfo($t('manage.verify_no_hashes'));
    return;
  }

  let passed = 0;
  const failedSnapshots: string[] = [];

  const activityId = addActivity({ title: $t('manage.verifying_archives'), status: 'running' });
  try {
    await withLoading(async () => {
      for (const snapshot of snapshots) {
        const result = await commands.verifyArchiveIntegrity(
          snapshot.path,
          snapshot.archive_hash ?? null
        );
        if (result.status === 'ok' && result.data) {
          passed++;
        } else {
          failedSnapshots.push(snapshot.date);
        }
      }
    }, $t('manage.verifying_archives'));
  } catch {
    updateActivity(activityId, { status: 'error' });
    return;
  }

  // Show results after loading overlay is dismissed
  if (failedSnapshots.length === 0) {
    updateActivity(activityId, {
      status: 'success',
      title: $t('manage.verify_all_passed', { count: passed }),
    });
  } else {
    updateActivity(activityId, { status: 'error', title: $t('manage.verify_failed_title') });
    const messageVNode = h('div', [
      h('p', $t('manage.verify_failed_summary', { passed, failed: failedSnapshots.length })),
      h(
        'ul',
        { style: 'max-height:200px;overflow-y:auto;padding-left:20px;margin:8px 0' },
        failedSnapshots.map((d) => h('li', { style: 'font-family:monospace;margin:2px 0' }, d))
      ),
      h('p', { style: 'color:#909399;font-size:12px' }, $t('manage.verify_failed_hint')),
    ]);
    try {
      await feedback.alert(messageVNode, $t('manage.verify_failed_title'), {
        type: 'error',
        confirmButtonText: $t('manage.verify_select_corrupted'),
      });
      // User clicked "Select corrupted" — select those snapshots in the table
      selectedDates.value = new Set(failedSnapshots);
    } catch {
      // dialog dismissed via close button
    }
  }
}

// 设置快速备份，由快捷键和tray触发备份和恢复
const isQuickBackupGame = computed(() => {
  return config.value.quick_action?.quick_action_game?.name === game.value.name;
});

async function set_quick_backup() {
  const result = await commands.setQuickBackupGame(game.value);
  if (result.status === 'error') {
    notifyError($t('manage.set_quick_backup_failed'));
    return;
  }
  await refreshConfig();
  notifySuccess($t('manage.set_quick_backup_success'));
}

// 处理抽屉组件保存游戏路径的事件
async function on_drawer_save_changes(updatedGame: Game) {
  try {
    const result = await commands.updateGame(game.value.storage_key ?? game.value.name, {
      name: updatedGame.name,
      save_paths: updatedGame.save_paths,
      game_paths: updatedGame.game_paths ?? {},
      ludusavi_meta: updatedGame.ludusavi_meta ?? null,
      device_bindings: updatedGame.device_bindings ?? {},
    });

    if (result.status === 'error') {
      notifyError(result.error);
      return;
    }

    await refreshConfig();
    notifySuccess($t('manage.save_paths_updated'));
    drawer.value = false;

    const currentRouteGameName = getGameNameFromRouteParam(
      'name' in route.params ? route.params.name : undefined
    );
    if (updatedGame.name !== currentRouteGameName) {
      await router.replace(getGameManagementPath(updatedGame.name));
    } else {
      const refreshedGame = config.value.games.find((g) => g.name === updatedGame.name);
      if (refreshedGame) {
        game.value = refreshedGame;
        await checkCurrentDeviceSavePaths();
      }
    }
  } catch (e) {
    error(`Error saving game paths: ${e}`);
    notifyError($t('error.save_config_failed'));
  }
}

const orderedTableData = computed(() =>
  tableSortBy.value.order === TableV2SortOrder.ASC ? table_data.value : table_data_desc.value
);

const filter_table = computed(() => {
  const keyword = search.value.trim();
  if (!keyword) {
    return orderedTableData.value;
  }

  return orderedTableData.value.filter(
    (data) => data.describe.includes(keyword) || data.date.includes(keyword)
  );
});

const selected_game_snapshots = computed<Snapshot[]>(() => {
  if (selectedDates.value.size === 0) {
    return [];
  }
  return filter_table.value.filter((snapshot) => selectedDates.value.has(snapshot.date));
});

const selectedCountInView = computed(() => {
  if (selectedDates.value.size === 0) {
    return 0;
  }
  let count = 0;
  for (const snapshot of filter_table.value) {
    if (selectedDates.value.has(snapshot.date)) {
      count += 1;
    }
  }
  return count;
});

const isAllSelected = computed(() => {
  const total = filter_table.value.length;
  return total > 0 && selectedCountInView.value === total;
});

const isSelectionIndeterminate = computed(() => {
  const total = filter_table.value.length;
  return total > 0 && selectedCountInView.value > 0 && selectedCountInView.value < total;
});

const tableColumns = computed(() => [
  { key: 'selection', dataKey: 'selection', title: '', width: 50, align: 'center' as const },
  {
    key: 'date',
    dataKey: 'date',
    title: $t('manage.save_date'),
    width: 190,
    sortable: true,
  },
  {
    key: 'describe',
    dataKey: 'describe',
    title: $t('manage.description'),
    width: 280,
    minWidth: 220,
    flexGrow: 1,
  },
  { key: 'size', dataKey: 'size', title: $t('manage.size'), width: 120 },
  {
    key: 'actions',
    dataKey: 'actions',
    title: $t('manage.actions'),
    width: 180,
    align: 'center' as const,
    fixed: TableV2FixedDir.RIGHT,
  },
]);

function onTableColumnSort({
  key,
  order,
}: {
  key: string | number | symbol;
  order: TableV2SortOrder;
}) {
  if (key !== 'date') return;
  tableSortBy.value = { key: 'date', order };
}

function isSnapshotSelected(date: string) {
  return selectedDates.value.has(date);
}

function toggleSnapshotSelection(date: string, checked: boolean) {
  const next = new Set(selectedDates.value);
  if (checked) {
    next.add(date);
  } else {
    next.delete(date);
  }
  selectedDates.value = next;
}

function toggleSelectAll(value: unknown) {
  const checked = value === true;
  if (checked) {
    selectedDates.value = new Set(filter_table.value.map((snapshot) => snapshot.date));
    return;
  }
  selectedDates.value = new Set();
}

watch(filter_table, (rows) => {
  if (selectedDates.value.size === 0) return;
  const visibleDates = new Set(rows.map((snapshot) => snapshot.date));
  const next = new Set<string>();
  let changed = false;

  for (const date of selectedDates.value) {
    if (visibleDates.has(date)) {
      next.add(date);
    } else {
      changed = true;
    }
  }

  if (changed) {
    selectedDates.value = next;
  }
});

// 检查当前设备的存档路径是否为空
async function checkCurrentDeviceSavePaths() {
  await fetchCurrentDevice();
  if (!currentDevice.value || !game.value || !game.value.save_paths) return;

  const enabledSaveUnits = game.value.save_paths.filter((unit) => unit.enabled !== false);
  if (enabledSaveUnits.length === 0) return;

  // 检查当前设备的存档路径是否全部为空
  const deviceId = currentDevice.value.id;
  const allPathsEmpty = enabledSaveUnits.every((unit) => {
    if (unit.source.type === 'manifestPattern') return unit.source.pattern.trim() === '';
    const paths = saveUnitPaths(unit);
    return !paths?.[deviceId]?.trim();
  });

  if (!allPathsEmpty) return; // 如果有路径不为空，直接返回

  // 收集所有有效的设备ID（有存档路径的设备）
  const devicesWithPaths = new Set<string>();
  enabledSaveUnits.forEach((unit) => {
    const paths = saveUnitPaths(unit);
    if (paths) {
      Object.entries(paths).forEach(([id, path]) => {
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
      if (unit.enabled === false) {
        return;
      }
      const paths = saveUnitPaths(unit);
      if (paths?.[sourceDeviceId]?.trim()) {
        if (!paths[targetDeviceId]?.trim()) {
          paths[targetDeviceId] = paths[sourceDeviceId];
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
        notifySuccess($t('manage.paths_copied_success'));
        // 打开侧栏让用户查看和编辑复制的路径
        drawer.value = true;
      } catch (e) {
        error(`Error saving config: ${e}`);
        notifyError($t('error.save_config_failed'));
      }
    }
  }
}

// Branch tree view handlers
async function onSetHead(date: string) {
  try {
    const result = await commands.setSnapshotHead(game.value, date);
    if (result.status === 'error') {
      notifyError($t('manage.set_head_failed'));
    } else {
      notifySuccess($t('manage.set_head_success'));
      await refresh_backups_info();
    }
  } catch (e) {
    error(`Failed to set HEAD: ${e}`);
    notifyError($t('manage.set_head_failed'));
  }
}

async function onDetach(date: string) {
  try {
    const result = await commands.detachSnapshot(game.value, date);
    if (result.status === 'error') {
      notifyError($t('manage.detach_failed'));
    } else {
      notifySuccess($t('manage.detach_success'));
      await refresh_backups_info();
    }
  } catch (e) {
    error(`Failed to detach snapshot: ${e}`);
    notifyError($t('manage.detach_failed'));
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
        notifyError(result.error);
      } else {
        notifySuccess(backupSuccessMessage());
        await refresh_backups_info();
      }
    }, $t('manage.creating_backup'));
  } catch {
    // User cancelled
  }
}

function resolveDeviceDisplayName(deviceId: string) {
  if (currentDevice.value?.id === deviceId && currentDevice.value.name.trim()) {
    return currentDevice.value.name;
  }

  const savedName = config.value?.devices?.[deviceId]?.name?.trim();
  if (savedName) {
    return savedName;
  }

  return deviceId.length > 8 ? `${deviceId.slice(0, 8)}...` : deviceId;
}

const deviceHeadMap = computed<Record<string, string>>(() => {
  const snapshots = gameSnapshots.value as GameSnapshotsWithDeviceHeads | null;
  return Object.fromEntries(
    Object.entries(snapshots?.device_heads ?? {}).filter(
      (entry): entry is [string, string] => typeof entry[1] === 'string' && entry[1].length > 0
    )
  );
});

const currentHead = computed(() => {
  const deviceId = currentDevice.value?.id;
  if (!deviceId) return null;
  return deviceHeadMap.value[deviceId] ?? null;
});

const headEntries = computed<DeviceHeadEntry[]>(() => {
  const currentDeviceId = currentDevice.value?.id;
  return Object.entries(deviceHeadMap.value)
    .map(([deviceId, date]) => {
      const snapshot = table_data.value.find((item) => item.date === date) ?? null;
      const description = snapshot?.describe?.trim() || '';
      const parsed = dayjs(date, 'YYYY-MM-DD_HH-mm-ss');
      const shortTime = parsed.isValid() ? parsed.format('MM/DD HH:mm') : date;
      const fullTime = parsed.isValid() ? parsed.format('YYYY-MM-DD HH:mm:ss') : date;
      const isCurrentDevice = deviceId === currentDeviceId;
      const label = isCurrentDevice
        ? $t('manage.current_position')
        : resolveDeviceDisplayName(deviceId);
      const fullText = description
        ? `${label} · ${description} (${fullTime})`
        : `${label} · ${fullTime}`;

      return {
        deviceId,
        deviceName: resolveDeviceDisplayName(deviceId),
        label,
        date,
        description,
        shortTime,
        fullText,
        isCurrentDevice,
      };
    })
    .sort((left, right) => {
      if (left.isCurrentDevice !== right.isCurrentDevice) {
        return left.isCurrentDevice ? -1 : 1;
      }
      return left.deviceName.localeCompare(right.deviceName);
    });
});

const branchDeviceHeads = computed<BranchDeviceHeadMarker[]>(() =>
  headEntries.value.map((entry) => ({
    deviceId: entry.deviceId,
    date: entry.date,
    label: entry.isCurrentDevice ? $t('manage.head') : entry.deviceName,
    isCurrentDevice: entry.isCurrentDevice,
    tooltip: entry.fullText,
  }))
);
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
        <el-tooltip :content="$t('manage.verify_archive_hashes')" placement="bottom">
          <el-button circle :icon="CircleCheck" @click="verify_archive_hashes" />
        </el-tooltip>
        <el-tooltip :content="$t('manage.extra_backups')" placement="bottom">
          <el-button circle :icon="DocumentCopy" @click="extraBackupDrawer = true" />
        </el-tooltip>
        <el-tooltip :content="$t('manage.show_drawer')" placement="bottom">
          <el-button circle :icon="Setting" @click="drawer = true" />
        </el-tooltip>
        <el-tooltip :content="$t('manage.set_quick_backup')" placement="bottom">
          <el-button
            circle
            :icon="Lightning"
            :type="isQuickBackupGame ? 'success' : ''"
            @click="set_quick_backup"
          />
        </el-tooltip>
        <el-tooltip :content="$t('manage.auto_save_settings')" placement="bottom">
          <el-button
            circle
            :icon="AlarmClock"
            :type="autoSaveConfigured ? 'success' : ''"
            @click="autoSaveSettingsDrawer = true"
          />
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
                :type="canUndo ? 'success' : ''"
                :icon="Back"
                :disabled="!canUndo"
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
          <div v-if="headEntries.length" class="head-tags">
            <el-tooltip
              v-for="entry in headEntries"
              :key="entry.deviceId"
              :content="entry.fullText"
              placement="bottom-end"
              :show-after="300"
              popper-class="head-tooltip"
            >
              <el-tag
                :type="entry.isCurrentDevice ? 'success' : 'info'"
                effect="plain"
                round
                :class="['head-tag', { 'head-tag--current': entry.isCurrentDevice }]"
              >
                <span class="head-label">{{ entry.label }}</span>
                <span class="head-separator">·</span>
                <span v-if="entry.description" class="head-desc">{{ entry.description }}</span>
                <span class="head-time">{{ entry.shortTime }}</span>
              </el-tag>
            </el-tooltip>
          </div>
        </div>
      </template>

      <!-- Table View -->
      <div v-if="viewMode === 'table'" class="view-container table-view">
        <el-empty v-if="filter_table.length === 0" :description="$t('manage.no_snapshots')" />
        <el-auto-resizer v-else>
          <template #default="{ height, width }">
            <el-table-v2
              :columns="tableColumns"
              :data="filter_table"
              :width="width"
              :height="height"
              row-key="date"
              :row-height="50"
              :header-height="44"
              :sort-by="tableSortBy"
              class="snapshot-table-v2"
              @column-sort="onTableColumnSort"
            >
              <template #header-cell="{ column }">
                <el-checkbox
                  v-if="column.key === 'selection'"
                  :model-value="isAllSelected"
                  :indeterminate="isSelectionIndeterminate"
                  @change="toggleSelectAll"
                />
                <span v-else>{{ column.title }}</span>
              </template>

              <template #cell="{ column, rowData }">
                <el-checkbox
                  v-if="column.key === 'selection'"
                  :model-value="isSnapshotSelected(rowData.date)"
                  @change="
                    (value: string | number | boolean) =>
                      toggleSnapshotSelection(rowData.date, value === true)
                  "
                />
                <span v-else-if="column.key === 'date'" class="font-mono text-sm">
                  {{ rowData.date }}
                </span>
                <el-tooltip
                  v-else-if="column.key === 'describe'"
                  :content="rowData.describe"
                  placement="top"
                  :show-after="300"
                  popper-class="action-tooltip"
                >
                  <span class="table-cell-describe">
                    <el-tag
                      v-if="snapshotSourceTag(rowData)"
                      type="info"
                      size="small"
                      effect="plain"
                      round
                      class="source-tag"
                      >{{ snapshotSourceTag(rowData) }}</el-tag
                    >
                    <span class="table-cell-ellipsis">{{ rowData.describe }}</span>
                  </span>
                </el-tooltip>
                <span v-else-if="column.key === 'size'" class="text-gray-500 text-xs">
                  {{ rowData.size ? formatFileSize(rowData.size) : '-' }}
                </span>
                <div v-else-if="column.key === 'actions'" class="action-buttons">
                  <el-tooltip
                    :content="$t('manage.apply')"
                    placement="top"
                    :show-after="300"
                    popper-class="action-tooltip"
                  >
                    <el-button
                      link
                      type="success"
                      :icon="VideoPlay"
                      @click="handleApplyClick(rowData.date)"
                    />
                  </el-tooltip>
                  <el-tooltip
                    v-if="isAutomaticSnapshot(rowData)"
                    :content="$t('manage.convert_to_permanent')"
                    placement="top"
                    :show-after="300"
                    popper-class="action-tooltip"
                  >
                    <el-button
                      link
                      type="primary"
                      :icon="Lock"
                      @click="convertToPermanent(rowData.date)"
                    />
                  </el-tooltip>
                  <el-tooltip
                    v-else
                    :content="$t('manage.change_describe')"
                    placement="top"
                    :show-after="300"
                    popper-class="action-tooltip"
                  >
                    <el-button
                      link
                      type="warning"
                      :icon="Edit"
                      @click="change_describe(rowData.date)"
                    />
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
                        @confirm="del_save(rowData.date)"
                      >
                        <template #reference>
                          <el-button link type="danger" :icon="Delete" />
                        </template>
                      </el-popconfirm>
                    </span>
                  </el-tooltip>
                </div>
              </template>
            </el-table-v2>
          </template>
        </el-auto-resizer>
      </div>

      <!-- Branch View -->
      <div v-else ref="branchViewContainer" class="view-container branch-view">
        <BranchTreeView
          v-if="viewMode === 'branch'"
          :snapshots="table_data"
          :current-head="currentHead"
          :device-heads="branchDeviceHeads"
          @apply="handleApplyClick"
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
    <AutoSaveSettingsDrawer
      v-if="game"
      v-model="autoSaveSettingsDrawer"
      :game="game"
      @saved="onAutoSaveSettingsSaved"
    />
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

.table-cell-describe {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  overflow: hidden;
}

.source-tag {
  flex-shrink: 0;
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
  align-items: flex-start;
  gap: 12px;
  flex-wrap: wrap;
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

.table-view {
  overflow: hidden;
}

.snapshot-table-v2 {
  width: 100%;
  height: 100%;
}

.snapshot-table-v2 :deep(.el-table-v2__header-cell),
.snapshot-table-v2 :deep(.el-table-v2__row-cell) {
  display: flex;
  align-items: center;
}

.snapshot-table-v2 :deep(.el-table-v2__header-cell .el-checkbox),
.snapshot-table-v2 :deep(.el-table-v2__row-cell .el-checkbox) {
  margin: 0 auto;
}

.table-cell-ellipsis {
  width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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

.head-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  justify-content: flex-end;
  margin-left: auto;
}

.head-tag {
  max-width: 280px;
  display: inline-flex;
  align-items: center;
  cursor: default;
  padding: 0 10px;
  box-sizing: border-box;
}

.head-tag--current .head-label {
  color: var(--el-color-success-dark-2);
}

.head-tag--current .head-separator {
  color: var(--el-color-success-light-3);
}

.head-label {
  flex-shrink: 0;
  color: var(--el-text-color-primary);
  font-weight: 500;
}

.head-separator {
  flex-shrink: 0;
  margin: 0 6px;
  color: var(--el-text-color-secondary);
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
