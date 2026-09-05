<script lang="ts" setup>
import { computed, ref, watch, onBeforeUnmount, onMounted, h } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import {
  commands,
  events,
  type CandidateDimensions,
  type CloudArchiveGameView,
  type Device,
  type Game,
  type GameSnapshots,
  type Snapshot,
} from '../../api/commands';
import SaveLocationDrawer from '../../components/SaveLocationDrawer.vue';
import AutoSaveSettingsDrawer from '../../components/AutoSaveSettingsDrawer.vue';
import BranchTreeView from '../../components/BranchTreeView.vue';
import ExtraBackupDrawer from '../../components/ExtraBackupDrawer.vue';
import SnapshotTable from '../../components/management/SnapshotTable.vue';
import { canApplySnapshot } from '../../components/management/snapshotAvailability';
import { useDeviceHeads, type DeviceHeadEntry } from '../../components/management/useDeviceHeads';
import { useSnapshotTransfers } from '../../components/management/useSnapshotTransfers';
import { $t } from '../../i18n';
import { error, info } from '../../utils/logger';
import {
  Copy,
  Download,
  Ellipsis,
  FolderCog,
  FolderMinus,
  FolderOpen,
  GitBranch,
  List,
  Play,
  Plus,
  RotateCcw,
  RotateCw,
  ShieldCheck,
  Timer,
  Undo2,
  Upload,
  Zap,
} from '@lucide/vue';
import dayjs from 'dayjs';
import {
  getGameManagementPath,
  getGameNameFromRouteParam,
} from '../../composables/useGameManagementRoute';
import { useApplyConfirmation } from '../../composables/useApplyConfirmation';
import { usePathResolution } from '../../composables/usePathResolution';
import { saveUnitPaths } from '../../utils/saveUnit';
import { KButton, KInput, KMenu, KSegmented, KTag, KTooltip, type KMenuEntry } from '../../ui/kit';

const { addActivity, updateActivity } = useActivityCenter();
const feedback = useFeedback();
const { config, refreshConfig, saveConfig } = useConfig();
const { confirmAndRun } = useApplyConfirmation();
const { markGamePlayed } = useSaveListSort();
const { withLoading } = useGlobalLoading();
const { startCollecting, stopCollecting } = useHostNotificationCollector();
const { preview: previewSaveUnit, rememberRestoreMapping } = usePathResolution();
const router = useRouter();
const route = useRoute();

// View mode: 'table' or 'branch'
const viewMode = ref<'table' | 'branch'>('table');

const search = ref(''); // 搜索时使用的字符串
const drawer = ref(false); // 是否显示存档位置侧栏
const extraBackupDrawer = ref(false);
const autoSaveSettingsDrawer = ref(false); // 是否显示自动保存设置抽屉

const table_data = ref<Snapshot[]>([]);
const table_data_desc = ref<Snapshot[]>([]);
const sortDesc = ref(true);
const selectedDates = ref<Set<string>>(new Set());
const retentionProtectedDates = ref<Set<string>>(new Set());
const cloudGame = ref<CloudArchiveGameView | null>(null);
const localCatalogDates = ref<Set<string>>(new Set());
const activeTransfer = ref('');

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

const pendingDeletions = computed(() => cloudGame.value?.pending_deletions ?? []);
async function batch_delete() {
  try {
    const ownership = await commands.getCurrentDeviceGameStatuses();
    if (ownership.status === 'error') {
      notifyError(ownership.error);
      return;
    }
    const global = ownership.data.some(
      (status) => status.game_id === (game.value.storage_key || game.value.name) && status.shared
    );
    const promptResult = await feedback.prompt(
      $t(global ? 'manage.batch_delete_global_prompt' : 'manage.batch_delete_prompt'),
      $t('home.hint'),
      {
        confirmButtonText: $t('manage.confirm'),
        cancelButtonText: $t('manage.cancel'),
        inputPattern: /yes/,
        inputErrorMessage: $t('manage.invalid_input_error'),
      }
    );

    if (promptResult.value === 'yes') {
      const dates = selected_game_snapshots.value.map((item) => item.date);
      if (global) {
        const gameId = game.value.storage_key || game.value.name;
        let succeeded = 0;
        for (const date of dates) {
          // When the batch includes the current head, pass the fallback
          // decision so the backend can move the position to the parent.
          // Refresh after each head deletion because the position may land
          // on the next snapshot in the batch.
          const isHead = currentHead.value === date;
          const result = isHead
            ? await commands.deleteV2Snapshot(gameId, date, true, {
                type: 'fallback_to_parent',
              })
            : await commands.deleteV2Snapshot(gameId, date, true);
          if (result.status === 'ok') {
            succeeded += 1;
            if (isHead) await refresh_backups_info();
          }
        }
        await refresh_backups_info();
        if (succeeded === dates.length) {
          notifySuccess($t('manage.batch_delete_success', { count: succeeded }));
        } else {
          notifyError(
            $t('manage.batch_delete_partial', {
              succeeded,
              failed: dates.length - succeeded,
            })
          );
        }
        return;
      }
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
    retentionProtectedDates.value = new Set();
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
    return;
  }
  const cloud = await loadCloudGame();
  const merged = mergeCloudOnlySnapshots(result.data.backups, cloud);
  localCatalogDates.value = new Set(result.data.backups.map((snapshot) => snapshot.date));
  gameSnapshots.value = { ...result.data, backups: merged };
  table_data.value = merged;
  table_data_desc.value = [...merged].reverse();
  selectedDates.value = new Set();
  retentionProtectedDates.value = new Set(
    cloud?.snapshots
      .filter((snapshot) => snapshot.retention_protected)
      .map((snapshot) => snapshot.snapshot_id) ?? []
  );
}

async function loadCloudGame() {
  const gameId = game.value.storage_key || game.value.name;
  if (!gameId) {
    cloudGame.value = null;
    return null;
  }
  const result = await commands.refreshCloudArchiveLibrary();
  if (result.status === 'error') {
    cloudGame.value = null;
    return null;
  }
  const next = result.data.games.find((item) => item.game_id === gameId) ?? null;
  cloudGame.value = next;
  return next;
}

function mergeCloudOnlySnapshots(
  local: Snapshot[],
  cloud: CloudArchiveGameView | null
): Snapshot[] {
  if (!cloud) return local;
  const known = new Set(local.map((snapshot) => snapshot.date));
  const extras = cloud.snapshots
    .filter((snapshot) => !known.has(snapshot.snapshot_id))
    .map((snapshot) => ({
      date: snapshot.snapshot_id,
      describe: snapshot.description,
      path: '',
      size: snapshot.size ?? 0,
      created_by: snapshot.created_by,
      parent: snapshot.parent,
    }));
  return [...local, ...extras].sort((left, right) => left.date.localeCompare(right.date));
}

const { currentHead, headEntries, branchDeviceHeads } = useDeviceHeads({
  gameSnapshots,
  tableData: table_data,
  currentDevice,
  config,
});

const {
  selectedUploadable,
  selectedDownloadable,
  selectedEvictable,
  transferSnapshot,
  retryPendingDeletion,
  evictSnapshot,
  evictCloudSnapshot,
  batchTransfer,
  batchEvict,
  convertToPermanent,
} = useSnapshotTransfers({
  game,
  cloudGame,
  localCatalogDates,
  activeTransfer,
  retentionProtectedDates,
  selected: () => selected_game_snapshots.value,
  allSnapshots: () => table_data.value,
  refresh: refresh_backups_info,
});

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

  backup_button_backup_limit = false;

  const activityId = addActivity({
    title: $t('manage.creating_backup'),
    status: 'running',
    acceptsStageUpdates: true,
  });
  try {
    await withLoading(
      async () => {
        const result = await commands.createSnapshotAt(game.value, describe.value, parentDate);
        if (result.status === 'error') {
          updateActivity(activityId, {
            status: 'error',
            title: $t('error.backup_failed'),
            description: result.error,
          });
        } else {
          updateActivity(activityId, { status: 'success', title: backupSuccessMessage() });
        }
      },
      $t('manage.creating_backup'),
      $t('manage.wait_for_prompt_hint')
    );
  } catch {
    updateActivity(activityId, { status: 'error', title: $t('error.backup_failed') });
  }
  backup_button_backup_limit = true;
  refresh_backups_info();

  describe.value = '';
}

async function create_new_save() {
  // Description is optional; the main path must stay one click.
  send_save_to_background();
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
    const ownership = await commands.getCurrentDeviceGameStatuses();
    if (ownership.status === 'error') {
      notifyError(ownership.error);
      return;
    }
    let result;
    if (
      ownership.data.some(
        (status) => status.game_id === (game.value.storage_key || game.value.name) && status.shared
      )
    ) {
      const gameId = game.value.storage_key || game.value.name;
      const snapshot = table_data.value.find((item) => item.date === date);
      await feedback.confirm(
        $t('sync_settings.archives.delete_confirm', {
          snapshot: snapshot?.describe || date,
        }),
        $t('sync_settings.archives.delete_title'),
        {
          confirmButtonText: $t('sync_settings.archives.delete_permanently'),
          cancelButtonText: $t('manage.cancel'),
          type: 'warning',
        }
      );
      if (currentHead.value === date) {
        const headSnapshot = table_data.value.find((item) => item.date === date);
        const parentDate = headSnapshot?.parent;
        const parentLabel = parentDate
          ? table_data.value.find((item) => item.date === parentDate)?.describe || parentDate
          : null;
        await feedback.confirm(
          parentLabel
            ? $t('sync_settings.archives.delete_head_fallback', { parent: parentLabel })
            : $t('sync_settings.archives.delete_head_clear'),
          $t('sync_settings.archives.delete_title'),
          {
            confirmButtonText: $t('sync_settings.archives.delete_permanently'),
            cancelButtonText: $t('manage.cancel'),
            type: 'warning',
          }
        );
        result = await commands.deleteV2Snapshot(gameId, date, true, {
          type: 'fallback_to_parent',
        });
      } else {
        result = await commands.deleteV2Snapshot(gameId, date, true);
      }
    } else {
      // 行内 popconfirm 移除后，本地命名空间也走统一破坏性确认
      await feedback.confirm($t('manage.confirm_delete_prompt'), $t('manage.delete'), {
        confirmButtonText: $t('manage.delete'),
        cancelButtonText: $t('manage.cancel'),
        type: 'warning',
      });
      result = await commands.deleteSnapshot(game.value, date);
    }
    if (result.status === 'error') {
      notifyError(result.error);
      return;
    }
    await refresh_backups_info();
    notifySuccess($t('manage.delete_success'));
  } catch (e) {
    info(`Snapshot deletion cancelled or interrupted: ${e}`);
  }
}

async function handleApplyClick(date: string) {
  if (!canApplySnapshot(localCatalogDates.value, cloudGame.value, date)) {
    notifyError($t('manage.download_before_apply'));
    return;
  }
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

  const activityId = addActivity({
    title: $t('manage.restoring_backup'),
    status: 'running',
    acceptsStageUpdates: true,
  });
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
    updateActivity(activityId, { status: 'error', title: $t('manage.recover_failed') });
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
        updateActivity(activityId, { status: 'error', title: $t('manage.undo_failed') });
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
    updateActivity(activityId, { status: 'error', title: $t('manage.undo_failed') });
  }

  refresh_backups_info();
}

async function change_describe(date: string) {
  if (!localCatalogDates.value.has(date)) return;
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
  const lastBackup = [...table_data.value]
    .reverse()
    .find((snapshot) => canApplySnapshot(localCatalogDates.value, cloudGame.value, snapshot.date));

  if (lastBackup?.date) {
    await confirmAndRun('latest', () => apply_save(lastBackup.date));
  } else {
    notifyError($t('manage.no_backup_error'));
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
    updateActivity(activityId, { status: 'error', title: $t('manage.verify_failed_title') });
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
  const identity = config.value.quick_action?.quick_action_game_id;
  return identity === game.value.storage_key || identity === game.value.name;
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
  sortDesc.value ? table_data_desc.value : table_data.value
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

function toggleSnapshotSelection(date: string, checked: boolean) {
  const next = new Set(selectedDates.value);
  if (checked) {
    next.add(date);
  } else {
    next.delete(date);
  }
  selectedDates.value = next;
}

function toggleSelectAll(checked: boolean) {
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
    // useFeedback 契约:确认则 resolve,取消则 reject
    await feedback.confirm($t('manage.empty_paths_prompt'), $t('manage.empty_paths_title'), {
      confirmButtonText: $t('manage.copy_from_device'),
      cancelButtonText: $t('manage.keep_empty'),
      type: 'info',
      closeOnClickModal: false,
      closeOnPressEscape: false,
    });

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

const syncParticipationLabel = computed(() => {
  const next = cloudGame.value;
  if (!next) return '';
  if (!next.managed) return $t('sync_settings.overview.unmanaged');
  if (!next.cloud_sync_enabled) return $t('sync_settings.overview.status_disabled');
  const mode = next.sync_mode;
  if (mode === 'multi_device_sync') {
    return $t('sync_settings.overview.mode_live');
  }
  if (mode === 'cloud_backup') {
    return $t('sync_settings.overview.mode_snapshot');
  }
  return $t('sync_settings.overview.mode_manual');
});

// 头部工具区：低频/破坏性动作收纳进溢出菜单，主按钮只留高频
const headerMenuEntries = computed<KMenuEntry[]>(() => [
  { type: 'item', key: 'openFolder', label: $t('manage.open_backup_folder'), icon: FolderOpen },
  { type: 'item', key: 'verify', label: $t('manage.verify_archive_hashes'), icon: ShieldCheck },
  { type: 'item', key: 'extraBackups', label: $t('manage.extra_backups'), icon: Copy },
  {
    type: 'item',
    key: 'quickBackup',
    label: $t('manage.set_quick_backup'),
    icon: Zap,
    active: isQuickBackupGame.value,
  },
  {
    type: 'item',
    key: 'autoSave',
    label: $t('manage.auto_save_settings'),
    icon: Timer,
    active: autoSaveConfigured.value,
  },
]);

function onHeaderMenuSelect(key: string) {
  if (key === 'openFolder') open_backup_folder();
  else if (key === 'verify') verify_archive_hashes();
  else if (key === 'extraBackups') extraBackupDrawer.value = true;
  else if (key === 'quickBackup') set_quick_backup();
  else if (key === 'autoSave') autoSaveSettingsDrawer.value = true;
}

const viewModeOptions = computed(() => [
  { value: 'table' as const, label: $t('manage.table_view'), icon: List },
  { value: 'branch' as const, label: $t('manage.branch_view'), icon: GitBranch },
]);
</script>

<template>
  <div class="flex h-[calc(100vh-40px)] flex-col gap-4 overflow-hidden">
    <!-- Page Header -->
    <div class="flex shrink-0 items-center justify-between gap-3">
      <div class="min-w-0 flex-1">
        <h2 class="truncate text-lg font-semibold text-text">{{ game.name }}</h2>
        <KTag v-if="cloudGame" class="mt-1">{{ syncParticipationLabel }}</KTag>
      </div>
      <div class="flex shrink-0 items-center gap-2">
        <KButton variant="default" @click="launch_game">
          <template #icon><Play :size="14" aria-hidden="true" /></template>
          {{ $t('manage.launch_game') }}
        </KButton>
        <KButton variant="default" @click="drawer = true">
          <template #icon><FolderCog :size="14" aria-hidden="true" /></template>
          {{ $t('manage.show_drawer') }}
        </KButton>
        <KMenu
          :entries="headerMenuEntries"
          :aria-label="$t('manage.more_actions')"
          @select="onHeaderMenuSelect"
        >
          <KButton variant="ghost" :aria-label="$t('manage.more_actions')">
            <template #icon><Ellipsis :size="16" aria-hidden="true" /></template>
          </KButton>
        </KMenu>
      </div>
    </div>

    <!-- Quick Actions -->
    <section
      class="flex shrink-0 items-center gap-3 rounded-md border border-border bg-surface p-3"
    >
      <KInput
        v-model="describe"
        class="flex-1"
        :placeholder="$t('manage.input_description_prompt')"
        :aria-label="$t('manage.input_description_prompt')"
        @keyup.enter="create_new_save"
      />
      <KButton variant="primary" @click="create_new_save">
        <template #icon><Plus :size="14" aria-hidden="true" /></template>
        {{ $t('manage.create_new_save') }}
      </KButton>
      <div class="h-6 w-px shrink-0 bg-border" aria-hidden="true" />
      <KButton variant="default" @click="load_latest_save">
        <template #icon><RotateCcw :size="14" aria-hidden="true" /></template>
        {{ $t('manage.load_latest_save') }}
      </KButton>
      <KTooltip :content="undoTooltip" side="bottom">
        <KButton
          variant="ghost"
          :aria-label="undoTooltip"
          :disabled="!canUndo"
          @click="undo_last_apply"
        >
          <template #icon><Undo2 :size="15" aria-hidden="true" /></template>
        </KButton>
      </KTooltip>
    </section>

    <!-- Main Content -->
    <section class="flex min-h-0 flex-1 flex-col rounded-md border border-border bg-surface">
      <div class="flex shrink-0 flex-wrap items-center gap-2 border-b border-border px-3 py-2.5">
        <KSegmented
          v-model="viewMode"
          :options="viewModeOptions"
          :aria-label="$t('manage.table_view')"
          class="w-52"
        />

        <KInput
          v-if="viewMode === 'table'"
          v-model="search"
          class="w-48"
          :placeholder="$t('manage.input_description_search_prompt')"
          :aria-label="$t('manage.input_description_search_prompt')"
        />

        <template v-if="viewMode === 'table'">
          <KButton v-if="selectedDownloadable.length > 0" size="sm" @click="batchTransfer(false)">
            <template #icon><Download :size="13" aria-hidden="true" /></template>
            {{ $t('manage.batch_download') }}
          </KButton>
          <KButton v-if="selectedEvictable.length > 0" size="sm" @click="batchEvict()">
            <template #icon><FolderMinus :size="13" aria-hidden="true" /></template>
            {{ $t('manage.batch_evict') }}
          </KButton>
          <KButton v-if="selectedUploadable.length > 0" size="sm" @click="batchTransfer(true)">
            <template #icon><Upload :size="13" aria-hidden="true" /></template>
            {{ $t('manage.batch_upload') }}
          </KButton>
          <KButton
            v-if="selected_game_snapshots.length > 0"
            size="sm"
            variant="danger"
            @click="batch_delete()"
          >
            {{ $t('manage.batch_delete') }}
          </KButton>
        </template>

        <div
          v-if="headEntries.length"
          class="ms-auto flex flex-wrap items-center justify-end gap-1.5"
        >
          <KTooltip
            v-for="entry in headEntries"
            :key="entry.deviceId"
            :content="entry.fullText"
            side="bottom"
          >
            <KTag :tone="entry.isCurrentDevice ? 'accent' : 'neutral'">
              <span class="font-semibold">{{ entry.label }}</span>
              <span v-if="entry.description" class="max-w-28 truncate">{{
                entry.description
              }}</span>
              <span class="font-mono text-[11px] opacity-80">{{ entry.shortTime }}</span>
            </KTag>
          </KTooltip>
        </div>
      </div>

      <div
        v-if="pendingDeletions.length"
        class="flex shrink-0 flex-col gap-2 border-b border-border px-3 py-2.5"
      >
        <div
          v-for="deletion in pendingDeletions"
          :key="deletion.snapshot_id"
          class="flex items-center justify-between gap-3 rounded-sm border border-[color-mix(in_oklab,var(--warning)_35%,transparent)] bg-[color-mix(in_oklab,var(--warning)_10%,transparent)] px-3 py-2"
        >
          <div class="min-w-0">
            <div class="truncate text-sm font-medium text-text">
              {{ deletion.description || deletion.snapshot_id }}
            </div>
            <div class="text-xs text-text-dim">
              {{ $t('sync_settings.archives.deletion_pending') }}
            </div>
          </div>
          <KButton
            v-if="deletion.retryable"
            size="sm"
            :loading="activeTransfer === deletion.snapshot_id"
            @click="retryPendingDeletion(deletion.snapshot_id, deletion.retryable)"
          >
            <template #icon><RotateCw :size="13" aria-hidden="true" /></template>
            {{ $t('sync_settings.archives.retry_delete') }}
          </KButton>
          <span v-else class="shrink-0 text-xs text-text-dim">{{
            $t('sync_settings.archives.deletion_waiting')
          }}</span>
        </div>
      </div>

      <!-- Table View -->
      <div v-if="viewMode === 'table'" class="h-full min-h-0 flex-1 overflow-hidden">
        <SnapshotTable
          :rows="filter_table"
          :sort-desc="sortDesc"
          :selected-dates="selectedDates"
          :cloud-game="cloudGame"
          :local-catalog-dates="localCatalogDates"
          :retention-protected-dates="retentionProtectedDates"
          :active-transfer="activeTransfer"
          @toggle-sort="sortDesc = !sortDesc"
          @toggle-select="toggleSnapshotSelection"
          @toggle-select-all="toggleSelectAll"
          @apply="handleApplyClick"
          @remove="del_save"
          @change-describe="change_describe"
          @convert-permanent="convertToPermanent"
          @evict="evictSnapshot"
          @evict-cloud="evictCloudSnapshot"
          @download="transferSnapshot($event, false)"
          @upload="transferSnapshot($event, true)"
        />
      </div>

      <!-- Branch View -->
      <div v-else class="h-full min-h-0 flex-1 overflow-hidden bg-surface-2">
        <BranchTreeView
          v-if="viewMode === 'branch'"
          :snapshots="table_data"
          :current-head="currentHead"
          :device-heads="branchDeviceHeads"
          :editable-dates="[...localCatalogDates]"
          @apply="handleApplyClick"
          @delete="del_save"
          @change-description="change_describe"
          @set-head="onSetHead"
          @detach="onDetach"
          @create-branch="onCreateBranch"
        />
      </div>
    </section>

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
      v-model="autoSaveSettingsDrawer"
      :game="game"
      :cloud-game="cloudGame"
      @saved="onAutoSaveSettingsSaved"
    />
  </div>
</template>
