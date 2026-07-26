<script setup lang="ts">
import { ref, computed, onMounted, watch, type Ref } from 'vue';
import { $t } from '../i18n';
import {
  commands,
  type Backend,
  type CloudBackendCheckReport,
  type CloudLibraryStatus,
  type ConflictResolution,
  type GameSyncState,
  type SyncState,
} from '../bindings';
import { error } from '@tauri-apps/plugin-log';
import { Download, Lock, Refresh, Upload, Warning } from '@element-plus/icons-vue';
import BackendCheckResult from '../components/BackendCheckResult.vue';
import CloudArchivePanel from '../components/CloudArchivePanel.vue';

interface WebDAV {
  type: 'WebDAV';
  endpoint: string;
  username: string;
  password: string;
}

interface S3 {
  type: 'S3';
  endpoint: string;
  bucket: string;
  region: string;
  access_key_id: string;
  secret_access_key: string;
  addressing_style: 'PathStyle' | 'VirtualHostedStyle' | 'Auto';
}

interface CloudSyncSessionConfig {
  root_path: string;
  max_concurrency: number;
  backend: Backend;
}

interface EditableCloudSettings {
  auto_sync_interval: number;
  root_path: string;
  backend: Backend;
  max_concurrency: number;
}

interface BatchSyncItemReportLike {
  name: string;
  status: unknown;
}

interface BatchSyncReportLike {
  config: BatchSyncItemReportLike;
  games: BatchSyncItemReportLike[];
}

const backends = [
  { value: 'WebDAV', label: 'sync_settings.backend_label.webdav' },
  { value: 'S3', label: 'sync_settings.backend_label.s3' },
  { value: 'Fs', label: 'sync_settings.backend_label.fs' },
  { value: 'Disabled', label: 'sync_settings.backend_label.disabled' },
] as const;

const { config, refreshConfig, saveConfig } = useConfig();
const { withLoading } = useGlobalLoading();
const feedback = useFeedback();

const activeTab = ref('overview');
const syncState = ref<SyncState | null>(null);
const syncingGames = ref<Set<string>>(new Set());
const syncingConfig = ref(false);
const resolvingConflict = ref(false);
const conflictDialogVisible = ref(false);
const selectedConflictGameName = ref<string | null>(null);
const checkingBackend = ref(false);
const backendCheckReport = ref<CloudBackendCheckReport | null>(null);
const cloudLibraryStatus = ref<CloudLibraryStatus | null>(null);
const cloud_settings = ref<EditableCloudSettings>(
  toEditableCloudSettings(config.value!.settings.cloud_settings)
);

const webdav_settings: Ref<WebDAV> = ref({
  type: 'WebDAV',
  endpoint: '',
  username: '',
  password: '',
} as WebDAV);
const s3_settings: Ref<S3> = ref({
  type: 'S3',
  endpoint: '',
  bucket: '',
  region: '',
  access_key_id: '',
  secret_access_key: '',
  addressing_style: 'PathStyle',
} as S3);

function cloneValue<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function toEditableCloudSettings(
  value: Partial<EditableCloudSettings> | undefined
): EditableCloudSettings {
  return {
    auto_sync_interval: value?.auto_sync_interval ?? 0,
    root_path: value?.root_path ?? '/game-save-manager',
    backend: cloneValue(value?.backend ?? ({ type: 'Disabled' } as Backend)),
    max_concurrency: value?.max_concurrency ?? 1,
  };
}

function loadBackendSettings() {
  switch (cloud_settings.value.backend.type) {
    case 'WebDAV':
      webdav_settings.value = cloneValue(cloud_settings.value.backend as WebDAV);
      break;
    case 'S3':
      s3_settings.value = cloneValue(cloud_settings.value.backend as S3);
      break;
    case 'Fs':
    case 'Disabled':
      break;
    default:
      notifyError($t('sync_settings.unknown_backend'));
      break;
  }
}

function loadDraftFromConfig() {
  cloud_settings.value = toEditableCloudSettings(config.value!.settings.cloud_settings);
  loadBackendSettings();
}

const savedBackendEnabled = computed(
  () => config.value?.settings.cloud_settings?.backend?.type !== 'Disabled'
);
const v2LibraryActive = computed(() => cloudLibraryStatus.value?.kind === 'active');
const snapshotSyncInterval = computed({
  get: () => cloud_settings.value.auto_sync_interval || 5,
  set: (minutes: number) => {
    cloud_settings.value.auto_sync_interval = minutes;
  },
});
const savedConnectionKey = computed(() =>
  JSON.stringify(config.value?.settings.cloud_settings ?? null)
);

function updateCloudLibraryStatus(status: CloudLibraryStatus | null) {
  cloudLibraryStatus.value = status;
}

const hasEnabledGames = computed(() =>
  (config.value?.games ?? []).some((game) => game.cloud_sync_enabled !== false)
);

interface GameRow {
  name: string;
  isConfig: boolean;
  cloudSyncEnabled: boolean;
  status: 'synced' | 'pending' | 'failed' | 'disabled' | 'conflict' | 'unknown';
  lastSyncAt: string | null;
  detail: string | null;
  syncState: GameSyncState | null;
}

/** Returns true when the error string indicates the bucket requires virtual-hosted-style addressing. */
function isVirtualHostStyleError(msg: string): boolean {
  return /virtual.host/i.test(msg);
}

function reportHasVirtualHostStyleError(report: CloudBackendCheckReport): boolean {
  return report.items.some((item) => item.message && isVirtualHostStyleError(item.message));
}

function resolveStatus(enabled: boolean, gs?: GameSyncState): GameRow['status'] {
  if (!enabled) return 'disabled';
  if (!gs) return 'unknown';
  if (gs.last_sync_result === 'cancelled') return 'pending';
  if (gs.pending_action === 'user_decision_required') return 'conflict';
  if (gs.pending_action === 'retry_required') return 'failed';
  if (!gs.last_sync_result) return 'pending';
  if (gs.last_sync_result === 'success') return 'synced';
  if (gs.last_sync_result === 'conflict') return 'conflict';
  if (typeof gs.last_sync_result === 'object' && 'error' in gs.last_sync_result) return 'failed';
  return 'unknown';
}

function syncErrorDetail(gs?: GameSyncState): string | null {
  const result = gs?.last_sync_result;
  if (result && typeof result === 'object' && 'error' in result) {
    return String(result.error);
  }
  return null;
}

const gameRows = computed<GameRow[]>(() => {
  const states = syncState.value?.games ?? {};
  const configState = syncState.value?.config_state;
  const configRow: GameRow = {
    name: $t('sync_settings.overview.config_row'),
    isConfig: true,
    cloudSyncEnabled: true,
    status: resolveStatus(savedBackendEnabled.value, configState),
    lastSyncAt: configState?.last_sync_at ?? null,
    detail: syncErrorDetail(configState),
    syncState: configState ?? null,
  };

  const rows: GameRow[] = (config.value?.games ?? []).map((game) => {
    const enabled = savedBackendEnabled.value && game.cloud_sync_enabled !== false;
    const gs: GameSyncState | undefined = states[game.name] ?? undefined;
    return {
      name: game.name,
      isConfig: false,
      cloudSyncEnabled: game.cloud_sync_enabled !== false,
      status: resolveStatus(enabled, gs),
      lastSyncAt: gs?.last_sync_at ?? null,
      detail: syncErrorDetail(gs),
      syncState: gs ?? null,
    };
  });

  return [configRow, ...rows];
});

type TagType = 'success' | 'warning' | 'danger' | 'info' | 'primary';
type StatusKey = GameRow['status'];
const STATUS_META: Record<StatusKey, { type: TagType; labelKey: string }> = {
  synced: { type: 'success', labelKey: 'sync_settings.overview.status_synced' },
  pending: { type: 'warning', labelKey: 'sync_settings.overview.status_pending' },
  failed: { type: 'danger', labelKey: 'sync_settings.overview.status_failed' },
  disabled: { type: 'info', labelKey: 'sync_settings.overview.status_disabled' },
  conflict: { type: 'warning', labelKey: 'sync_settings.overview.status_conflict' },
  unknown: { type: 'info', labelKey: 'sync_settings.overview.status_unknown' },
};

function statusLabel(status: StatusKey) {
  return $t(STATUS_META[status]?.labelKey ?? status);
}

function statusType(status: StatusKey): TagType {
  return STATUS_META[status]?.type ?? 'info';
}

const selectedConflictRow = computed(
  () =>
    gameRows.value.find((row) => !row.isConfig && row.name === selectedConflictGameName.value) ??
    null
);
const selectedConflictState = computed(() => selectedConflictRow.value?.syncState ?? null);

function openConflictDialog(row: GameRow) {
  if (row.isConfig || row.status !== 'conflict') return;
  selectedConflictGameName.value = row.name;
  conflictDialogVisible.value = true;
}

function tableRowToGameRow(row: unknown): GameRow {
  return row as GameRow;
}

async function retryConfigSync() {
  if (!savedBackendEnabled.value || syncingConfig.value) return;
  syncingConfig.value = true;
  try {
    const result = await commands.syncConfig();
    if (result.status === 'error') {
      notifyError(`${$t('sync_settings.config_sync_failed')}: ${result.error}`);
      error(`Sync config error: ${result.error}`);
      return;
    }
    notifySuccess($t('sync_settings.config_sync_success'));
  } catch (e) {
    error(`Sync config exception: ${e}`);
    notifyError(String(e));
  } finally {
    syncingConfig.value = false;
    await loadSyncState();
  }
}

async function resolveConflict(resolution: ConflictResolution) {
  const row = selectedConflictRow.value;
  if (!row || row.isConfig) return;

  const confirmKey =
    resolution === 'keep_local'
      ? 'sync_settings.conflict.keep_local_confirm'
      : 'sync_settings.conflict.accept_remote_confirm';

  try {
    await feedback.confirm($t(confirmKey, { game: row.name }), $t('sync_settings.conflict.title'), {
      confirmButtonText: $t('sync_settings.confirm'),
      cancelButtonText: $t('sync_settings.cancel'),
      type: 'warning',
    });
  } catch {
    notifyInfo($t('sync_settings.canceled'), undefined, { silent: true });
    return;
  }

  try {
    resolvingConflict.value = true;
    const result = await commands.resolveGameSyncConflict(row.name, resolution);
    if (result.status === 'error') {
      notifyError(`${$t('sync_settings.conflict.resolve_failed')}: ${result.error}`);
      error(`Resolve conflict error for ${row.name}: ${result.error}`);
      return;
    }

    notifySuccess($t('sync_settings.conflict.resolve_success'));
    conflictDialogVisible.value = false;
  } catch (e) {
    error(`Resolve conflict exception for ${row.name}: ${e}`);
    notifyError(`${$t('sync_settings.conflict.resolve_failed')}: ${String(e)}`);
  } finally {
    resolvingConflict.value = false;
    await loadSyncState();
  }
}

async function toggleGameSync(row: GameRow) {
  if (row.isConfig) return;
  const game = config.value?.games.find((item) => item.name === row.name);
  if (!game) return;

  const previous = game.cloud_sync_enabled !== false;
  game.cloud_sync_enabled = row.cloudSyncEnabled;

  const saved = await saveConfig();
  if (!saved) {
    game.cloud_sync_enabled = previous;
    row.cloudSyncEnabled = previous;
    error(`Failed to toggle sync for ${row.name}`);
    return;
  }

  await loadSyncState();
}

function formatTime(iso: string | null): string {
  if (!iso) return '—';
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleString();
}

async function loadSyncState() {
  try {
    const result = await commands.getSyncState();
    if (result.status === 'ok') {
      syncState.value = result.data;
    } else {
      syncState.value = null;
      error(`Failed to load sync state: ${result.error}`);
    }
  } catch (e) {
    syncState.value = null;
    error(`Exception loading sync state: ${e}`);
  }
}

async function syncGame(gameName: string) {
  if (!savedBackendEnabled.value || syncingGames.value.has(gameName)) return;
  syncingGames.value.add(gameName);
  try {
    const result = await commands.syncGame(gameName);
    if (result.status === 'error') {
      notifyError(`${$t('sync_settings.sync_failed')}: ${result.error}`);
      error(`Sync game error for ${gameName}: ${result.error}`);
      if (isVirtualHostStyleError(result.error)) {
        notifyWarning($t('sync_settings.s3.virtual_host_hint'));
      }
      return;
    }

    if (result.data === 'conflict') {
      notifyWarning(statusLabel('conflict'));
    } else {
      notifySuccess($t('sync_settings.sync_success'));
    }
  } catch (e) {
    error(`Sync game exception for ${gameName}: ${e}`);
    notifyError(String(e));
  } finally {
    syncingGames.value.delete(gameName);
    await loadSyncState();
  }
}

async function syncAllGames() {
  if (!savedBackendEnabled.value) return;
  const enabledGames = (config.value?.games ?? []).filter(
    (game) => game.cloud_sync_enabled !== false
  );
  if (enabledGames.length === 0) return;

  await withLoading(async () => {
    for (const game of enabledGames) {
      await syncGame(game.name);
    }
  }, $t('sync_settings.overview.syncing_all'));
}

watch(activeTab, (tab) => {
  if (tab === 'overview') {
    void loadSyncState();
  }
});

watch(
  [cloud_settings, webdav_settings, s3_settings],
  () => {
    backendCheckReport.value = null;
  },
  { deep: true }
);

function trimEndpoint(settings: { endpoint: string }) {
  if (settings.endpoint.endsWith('/')) {
    settings.endpoint = settings.endpoint.slice(0, -1);
  }
}

function currentBackend(): Backend | null {
  const type = cloud_settings.value?.backend?.type;
  if (type === 'WebDAV') {
    trimEndpoint(webdav_settings.value);
    return cloneValue(webdav_settings.value);
  }
  if (type === 'S3') {
    trimEndpoint(s3_settings.value);
    return cloneValue(s3_settings.value);
  }
  if (type === 'Fs') {
    return { type: 'Fs' } as Backend;
  }
  if (type === 'Disabled') {
    return { type: 'Disabled' } as Backend;
  }
  return null;
}

async function chooseFsRoot() {
  const result = await commands.chooseSaveDir();
  if (result.status === 'ok') {
    cloud_settings.value.root_path = result.data;
  }
}

function currentSessionConfig(): CloudSyncSessionConfig | null {
  const backend = currentBackend();
  if (!backend) return null;
  return {
    root_path: cloud_settings.value.root_path,
    max_concurrency: cloud_settings.value.max_concurrency,
    backend,
  };
}

async function check() {
  backendCheckReport.value = null;
  checkingBackend.value = true;
  try {
    const session = currentSessionConfig();
    if (!session || session.backend.type === 'Disabled') {
      notifyError($t('sync_settings.test_failed'));
      return;
    }
    const result = await withLoading(async () => {
      return await commands.checkCloudBackend(session);
    }, $t('sync_settings.checking_backend'));
    if (result.status === 'error') {
      notifyError($t('sync_settings.test_failed'));
      error(`${session.backend.type} test error: ${result.error}`);
      if (isVirtualHostStyleError(result.error)) {
        notifyWarning($t('sync_settings.s3.virtual_host_hint'));
      }
      return;
    }

    backendCheckReport.value = result.data;
    if (session.backend.type === 'S3' && reportHasVirtualHostStyleError(result.data)) {
      notifyWarning($t('sync_settings.s3.virtual_host_hint'));
    }
  } finally {
    checkingBackend.value = false;
  }
}

async function save() {
  const backend = currentBackend();
  if (!backend) {
    notifyError($t('sync_settings.unknown_backend'));
    return;
  }

  config.value!.settings.cloud_settings = {
    ...cloneValue(cloud_settings.value),
    backend,
  };

  await submit_settings();
}

async function load_config() {
  const loaded = await refreshConfig();
  if (loaded) {
    loadDraftFromConfig();
  }
  return loaded;
}

async function submit_settings() {
  const saved = await saveConfig();
  if (!saved) {
    error('Failed to set config');
    return;
  }
  await load_config();
  notifySuccess($t('sync_settings.submit_success'));
}

async function abort_change() {
  const loaded = await load_config();
  if (loaded) {
    notifySuccess($t('sync_settings.reset_success'));
  }
}

function isFailedBatchStatus(status: unknown): boolean {
  return !!status && typeof status === 'object' && 'failed' in (status as Record<string, unknown>);
}

function isCancelledBatchStatus(status: unknown): boolean {
  return status === 'cancelled';
}

function reportHasFailures(report: BatchSyncReportLike): boolean {
  return [report.config.status, ...report.games.map((item) => item.status)].some(
    isFailedBatchStatus
  );
}

function reportWasCancelled(report: BatchSyncReportLike): boolean {
  return [report.config.status, ...report.games.map((item) => item.status)].some(
    isCancelledBatchStatus
  );
}

async function upload_all() {
  try {
    await feedback.prompt($t('sync_settings.confirm_upload_all'), $t('home.hint'), {
      confirmButtonText: $t('sync_settings.confirm'),
      cancelButtonText: $t('sync_settings.cancel'),
      inputPattern: /yes/,
      inputErrorMessage: $t('sync_settings.invalid_input_error'),
    });

    const session = currentSessionConfig();
    if (!session || session.backend.type === 'Disabled') {
      notifyError($t('sync_settings.upload_failed'));
      return;
    }

    const result = await withLoading(async () => {
      return await commands.cloudUploadAll(session);
    }, $t('sync_settings.uploading_all'));

    if (result.status === 'error') {
      notifyError($t('sync_settings.upload_failed'));
      error(`Upload error: ${result.error}`);
      if (isVirtualHostStyleError(result.error)) {
        notifyWarning($t('sync_settings.s3.virtual_host_hint'));
      }
    } else if (reportHasFailures(result.data as BatchSyncReportLike)) {
      notifyError($t('sync_settings.upload_failed'));
    } else if (reportWasCancelled(result.data as BatchSyncReportLike)) {
      notifyInfo($t('sync_settings.canceled'), undefined, { silent: true });
    } else {
      notifySuccess($t('sync_settings.upload_success'));
    }
  } catch {
    notifyInfo($t('sync_settings.canceled'), undefined, { silent: true });
  } finally {
    await loadSyncState();
  }
}

async function download_all() {
  try {
    await feedback.prompt($t('sync_settings.confirm_download_all'), $t('home.hint'), {
      confirmButtonText: $t('sync_settings.confirm'),
      cancelButtonText: $t('sync_settings.cancel'),
      inputPattern: /yes/,
      inputErrorMessage: $t('sync_settings.invalid_input_error'),
    });

    const session = currentSessionConfig();
    if (!session || session.backend.type === 'Disabled') {
      notifyError($t('sync_settings.download_failed'));
      return;
    }

    const result = await withLoading(async () => {
      return await commands.cloudDownloadAll(session);
    }, $t('sync_settings.downloading_all'));

    if (result.status === 'error') {
      notifyError($t('sync_settings.download_failed'));
      error(`Download error: ${result.error}`);
      if (isVirtualHostStyleError(result.error)) {
        notifyWarning($t('sync_settings.s3.virtual_host_hint'));
      }
    } else if (reportHasFailures(result.data as BatchSyncReportLike)) {
      notifyError($t('sync_settings.download_failed'));
    } else if (reportWasCancelled(result.data as BatchSyncReportLike)) {
      notifyInfo($t('sync_settings.canceled'), undefined, { silent: true });
    } else {
      notifySuccess($t('sync_settings.download_success'));
      await load_config();
    }
  } catch {
    notifyInfo($t('sync_settings.canceled'), undefined, { silent: true });
  } finally {
    await loadSyncState();
  }
}

async function cancelSync() {
  const result = await commands.cancelCloudSync();
  if (result.status === 'error') {
    notifyError(result.error);
    error(`Cancel sync error: ${result.error}`);
    return;
  }

  if (result.data === 'cancelled') {
    notifyInfo($t('cloud_sync.cancelled'), undefined, { silent: true });
  }
}

async function open_manual() {
  const result = await commands.openUrl('https://help.sworld.club/docs/extras/cloud');
  if (result.status === 'error') {
    error(`open manual error: ${result.error}`);
    notifyError($t('error.open_url_failed'));
  }
}

onMounted(async () => {
  await load_config();
  await loadSyncState();
});
</script>

<template>
  <div class="sync-page">
    <div class="page-header">
      <h2 class="page-title">{{ $t('sync_settings.title') }}</h2>
      <ElLink type="primary" @click="open_manual">{{ $t('sync_settings.manual_link') }}</ElLink>
    </div>

    <ElTabs v-model="activeTab" class="sync-tabs">
      <!-- Tab 1: Overview -->
      <ElTabPane :label="$t('sync_settings.overview.tab')" name="overview">
        <ElAlert
          v-if="v2LibraryActive"
          type="success"
          :title="$t('sync_settings.library.legacy_disabled')"
          :closable="false"
          show-icon
          class="section-alert"
        />
        <CloudArchivePanel v-if="v2LibraryActive" />
        <div v-if="!v2LibraryActive" class="overview-toolbar">
          <ElButton
            type="primary"
            :icon="Refresh"
            :disabled="v2LibraryActive || !savedBackendEnabled || !hasEnabledGames"
            @click="syncAllGames"
          >
            {{ $t('sync_settings.overview.sync_all') }}
          </ElButton>
        </div>

        <ElTable
          v-if="!v2LibraryActive"
          :data="gameRows"
          stripe
          class="game-table"
          :empty-text="$t('sync_settings.overview.no_games')"
        >
          <ElTableColumn
            prop="name"
            :label="$t('sync_settings.overview.game_name')"
            min-width="180"
          >
            <template #default="{ row }">
              <div class="game-name-cell">
                <div class="game-name-stack">
                  <div class="game-title-line">
                    <span class="game-name">{{ row.name }}</span>
                    <ElTag v-if="row.isConfig" size="small" type="info" round effect="plain">
                      <ElIcon :size="12" style="margin-right: 2px"><Lock /></ElIcon>
                      {{ $t('sync_settings.overview.always_synced') }}
                    </ElTag>
                  </div>
                  <span v-if="row.detail" class="row-detail">{{ row.detail }}</span>
                </div>
              </div>
            </template>
          </ElTableColumn>
          <ElTableColumn
            :label="$t('sync_settings.overview.cloud_sync')"
            width="100"
            align="center"
          >
            <template #default="{ row }">
              <ElSwitch
                v-if="!row.isConfig"
                v-model="row.cloudSyncEnabled"
                size="small"
                :disabled="v2LibraryActive"
                @change="toggleGameSync(tableRowToGameRow(row))"
              />
              <span v-else class="config-lock-icon">
                <ElIcon><Lock /></ElIcon>
              </span>
            </template>
          </ElTableColumn>
          <ElTableColumn :label="$t('sync_settings.overview.status')" width="120" align="center">
            <template #default="{ row }">
              <ElTag
                :type="statusType(row.status)"
                size="small"
                effect="light"
                round
                disable-transitions
              >
                {{ statusLabel(row.status) }}
              </ElTag>
            </template>
          </ElTableColumn>
          <ElTableColumn :label="$t('sync_settings.overview.last_sync')" width="180" align="center">
            <template #default="{ row }">
              <span class="time-text">{{ formatTime(row.lastSyncAt) }}</span>
            </template>
          </ElTableColumn>
          <ElTableColumn :label="$t('sync_settings.overview.actions')" width="150" align="center">
            <template #default="{ row }">
              <ElButton
                v-if="
                  row.isConfig && row.status === 'failed' && savedBackendEnabled && !v2LibraryActive
                "
                :icon="Refresh"
                size="small"
                text
                :loading="syncingConfig"
                @click="retryConfigSync"
              >
                {{ $t('sync_settings.config_retry') }}
              </ElButton>
              <ElButton
                v-else-if="!v2LibraryActive && !row.isConfig && row.status === 'conflict'"
                :icon="Warning"
                type="warning"
                size="small"
                text
                @click="openConflictDialog(tableRowToGameRow(row))"
              >
                {{ $t('sync_settings.conflict.resolve') }}
              </ElButton>
              <ElButton
                v-else-if="
                  !v2LibraryActive && !row.isConfig && row.cloudSyncEnabled && savedBackendEnabled
                "
                :icon="Refresh"
                size="small"
                text
                :loading="syncingGames.has(row.name)"
                @click="syncGame(row.name)"
              />
            </template>
          </ElTableColumn>
        </ElTable>
      </ElTabPane>

      <!-- Tab 2: Backend -->
      <ElTabPane :label="$t('sync_settings.backend_tab.tab')" name="backend">
        <ElAlert type="warning" :closable="false" show-icon style="margin-bottom: 20px">
          {{ $t('sync_settings.warning') }}
        </ElAlert>
        <div class="backend-layout">
          <ElForm label-position="left" :label-width="160" class="backend-form">
            <ElFormItem :label="$t('sync_settings.backend')">
              <ElSelect
                v-model="cloud_settings!.backend!.type"
                :placeholder="$t('sync_settings.backend')"
              >
                <ElOption
                  v-for="backend in backends"
                  :key="backend.value"
                  :label="$t(backend.label)"
                  :value="backend.value"
                />
              </ElSelect>
            </ElFormItem>

            <!-- WebDAV -->
            <template v-if="cloud_settings!.backend!.type === 'WebDAV'">
              <ElFormItem :label="$t('sync_settings.webdav.endpoint')">
                <ElInput v-model="webdav_settings.endpoint" />
              </ElFormItem>
              <ElFormItem :label="$t('sync_settings.webdav.username')">
                <ElInput v-model="webdav_settings.username" />
              </ElFormItem>
              <ElFormItem :label="$t('sync_settings.webdav.password')">
                <ElInput v-model="webdav_settings.password" type="password" show-password />
              </ElFormItem>
            </template>

            <!-- S3 -->
            <template v-if="cloud_settings!.backend!.type === 'S3'">
              <ElFormItem :label="$t('sync_settings.s3.endpoint')">
                <ElInput v-model="s3_settings.endpoint" />
              </ElFormItem>
              <ElFormItem :label="$t('sync_settings.s3.bucket')">
                <ElInput v-model="s3_settings.bucket" />
              </ElFormItem>
              <ElFormItem :label="$t('sync_settings.s3.region')">
                <ElInput v-model="s3_settings.region" />
                <span class="field-hint">{{ $t('sync_settings.s3.region_hint') }}</span>
              </ElFormItem>
              <ElFormItem :label="$t('sync_settings.s3.access_key_id')">
                <ElInput v-model="s3_settings.access_key_id" />
              </ElFormItem>
              <ElFormItem :label="$t('sync_settings.s3.secret_access_key')">
                <ElInput v-model="s3_settings.secret_access_key" type="password" show-password />
              </ElFormItem>
              <ElFormItem :label="$t('sync_settings.s3.addressing_style')">
                <ElSelect v-model="s3_settings.addressing_style">
                  <ElOption
                    value="PathStyle"
                    :label="$t('sync_settings.s3.addressing_style_path')"
                  />
                  <ElOption
                    value="VirtualHostedStyle"
                    :label="$t('sync_settings.s3.addressing_style_virtual')"
                  />
                  <ElOption value="Auto" :label="$t('sync_settings.s3.addressing_style_auto')" />
                </ElSelect>
              </ElFormItem>
            </template>

            <ElFormItem
              v-if="cloud_settings!.backend!.type === 'Fs'"
              :label="$t('sync_settings.fs.root')"
            >
              <ElInput v-model="cloud_settings!.root_path">
                <template #append>
                  <ElButton @click="chooseFsRoot">
                    {{ $t('sync_settings.fs.choose') }}
                  </ElButton>
                </template>
              </ElInput>
              <span class="field-hint">{{ $t('sync_settings.fs.root_hint') }}</span>
            </ElFormItem>
            <ElFormItem v-else :label="$t('sync_settings.cloud_root')">
              <ElInput v-model="cloud_settings!.root_path" />
              <span class="field-hint">{{ $t('sync_settings.cloud_root_hint') }}</span>
            </ElFormItem>
            <ElFormItem :label="$t('sync_settings.max_concurrency')">
              <ElInputNumber
                v-model="cloud_settings!.max_concurrency"
                :value-on-clear="1"
                :step="1"
                :step-strictly="true"
                :min="1"
                :max="32"
              />
              <span class="field-hint">{{ $t('sync_settings.max_concurrency_hint') }}</span>
            </ElFormItem>
            <ElFormItem v-if="v2LibraryActive" :label="$t('sync_settings.auto_sync_interval')">
              <ElInputNumber
                v-model="snapshotSyncInterval"
                :step="1"
                :step-strictly="true"
                :min="1"
                :max="1440"
              />
            </ElFormItem>

            <ElFormItem>
              <div class="button-group">
                <ElButton type="primary" @click="save">
                  {{ $t('sync_settings.save_button') }}
                </ElButton>
                <ElButton @click="abort_change">
                  {{ $t('sync_settings.abort_button') }}
                </ElButton>
                <ElButton
                  :disabled="currentSessionConfig()?.backend.type === 'Disabled'"
                  :loading="checkingBackend"
                  @click="check"
                >
                  {{ $t('sync_settings.test_button') }}
                </ElButton>
              </div>
            </ElFormItem>
          </ElForm>

          <BackendCheckResult :report="backendCheckReport" :checking="checkingBackend" />
        </div>
        <CloudLibrarySetup
          class="library-setup"
          :enabled="savedBackendEnabled"
          :connection-key="savedConnectionKey"
          @status="updateCloudLibraryStatus"
        />
      </ElTabPane>

      <!-- Tab 3: Operations -->
      <ElTabPane :label="$t('sync_settings.operations.tab')" name="operations">
        <ElAlert
          v-if="v2LibraryActive"
          type="success"
          :title="$t('sync_settings.library.legacy_disabled')"
          :closable="false"
          show-icon
          class="section-alert"
        />
        <ElAlert type="warning" :closable="false" show-icon style="margin-bottom: 20px">
          {{ $t('sync_settings.operations.warning') }}
        </ElAlert>
        <div class="operations-list">
          <div class="op-item">
            <div class="op-info">
              <h4>{{ $t('sync_settings.overwrite_upload') }}</h4>
              <p class="op-desc">{{ $t('sync_settings.operations.upload_desc') }}</p>
            </div>
            <ElButton
              type="danger"
              :icon="Upload"
              :disabled="v2LibraryActive || currentSessionConfig()?.backend.type === 'Disabled'"
              @click="upload_all"
            >
              {{ $t('sync_settings.overwrite_upload') }}
            </ElButton>
          </div>
          <ElDivider />
          <div class="op-item">
            <div class="op-info">
              <h4>{{ $t('sync_settings.overwrite_download') }}</h4>
              <p class="op-desc">{{ $t('sync_settings.operations.download_desc') }}</p>
            </div>
            <ElButton
              type="danger"
              :icon="Download"
              :disabled="v2LibraryActive || currentSessionConfig()?.backend.type === 'Disabled'"
              @click="download_all"
            >
              {{ $t('sync_settings.overwrite_download') }}
            </ElButton>
          </div>
          <ElDivider />
          <div class="op-item">
            <div class="op-info">
              <h4>{{ $t('sync_settings.operations.cancel_sync') }}</h4>
              <p class="op-desc">{{ $t('sync_settings.operations.cancel_sync_desc') }}</p>
            </div>
            <ElButton type="warning" @click="cancelSync">
              {{ $t('sync_settings.operations.cancel_sync') }}
            </ElButton>
          </div>
        </div>
      </ElTabPane>
    </ElTabs>

    <SyncConflictDialog
      v-model="conflictDialogVisible"
      :game-name="selectedConflictRow?.name ?? ''"
      :state="selectedConflictState"
      :current-device-id="syncState?.current_device_id"
      :resolving="resolvingConflict"
      @resolve="resolveConflict"
    />
  </div>
</template>

<style scoped>
.sync-page {
  padding: 0 8px;
}

.page-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 20px;
  padding-bottom: 16px;
  border-bottom: 1px solid var(--el-border-color-lighter);
}

.page-title {
  margin: 0;
  font-size: 1.4em;
  font-weight: 600;
  color: var(--el-text-color-primary);
}

/* Tabs */
.sync-tabs {
  :deep(.el-tabs__header) {
    margin-bottom: 16px;
  }

  :deep(.el-tabs__nav-wrap::after) {
    height: 1px;
  }
}

.section-alert,
.library-setup {
  margin-bottom: 20px;
}

/* Overview tab */
.overview-toolbar {
  display: flex;
  gap: 8px;
  margin-bottom: 16px;
}

.game-table {
  border-radius: 8px;
  overflow: hidden;

  :deep(.el-table__header th) {
    background-color: var(--el-fill-color-light);
    font-weight: 600;
    color: var(--el-text-color-primary);
  }

  :deep(.el-table__row) {
    transition: background-color 0.3s ease;
  }

  :deep(.el-table__row:hover > td) {
    background-color: var(--el-fill-color-light) !important;
  }

  /* Config row highlight */
  :deep(.el-table__body tr:first-child td) {
    background-color: var(--el-fill-color-lighter);
  }

  :deep(.el-table__body tr:first-child:hover td) {
    background-color: var(--el-fill-color-light) !important;
  }

  :deep(.el-table__empty-block) {
    min-height: 120px;
  }

  :deep(.el-tag) {
    font-weight: 500;
    letter-spacing: 0.02em;
  }
}

.game-name-cell {
  display: flex;
  align-items: center;
  gap: 8px;
}

.game-name-stack {
  display: grid;
  gap: 4px;
  min-width: 0;
}

.game-title-line {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.game-name {
  font-weight: 500;
}

.row-detail {
  color: var(--el-text-color-secondary);
  font-size: 0.78em;
  line-height: 1.35;
  word-break: break-word;
}

.config-lock-icon {
  color: var(--el-text-color-secondary);
}

.time-text {
  color: var(--el-text-color-secondary);
  font-size: 0.85em;
  font-variant-numeric: tabular-nums;
}

/* Backend tab */
.backend-layout {
  max-width: 560px;
}

.backend-form .el-input,
.backend-form .el-select {
  width: 320px;
}

.field-hint {
  display: block;
  margin-top: 4px;
  color: var(--el-text-color-placeholder);
  font-size: 0.85em;
  line-height: 1.4;
}

.button-group {
  display: flex;
  gap: 12px;
  padding-top: 8px;
}

@media (max-width: 640px) {
  .backend-layout {
    max-width: none;
  }

  .backend-form .el-input,
  .backend-form .el-select {
    width: 100%;
  }

  .button-group {
    flex-wrap: wrap;
  }
}

/* Operations tab */
.operations-list {
  max-width: 600px;
}

.op-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 24px;
}

.op-item :deep(.el-button) {
  flex-shrink: 0;
  white-space: nowrap;
}

.op-info h4 {
  margin: 0 0 4px 0;
  font-size: 0.95em;
}

.op-desc {
  margin: 0;
  color: var(--el-text-color-secondary);
  font-size: 0.85em;
  line-height: 1.4;
}
</style>
