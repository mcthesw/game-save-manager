<script setup lang="ts">
import { ref, computed, onMounted, watch, type Ref } from 'vue';
import { $t } from '../i18n';
import { commands, type Backend, type GameSyncState, type SyncState } from '../bindings';
import { error } from '@tauri-apps/plugin-log';
import { Lock, Refresh, Upload, Download } from '@element-plus/icons-vue';

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

const backends = ['WebDAV', 'S3', 'Disabled'];

const { config, refreshConfig, saveConfig } = useConfig();
const { showInfo, showWarning, showError, showSuccess } = useNotification();
const { withLoading } = useGlobalLoading();
const feedback = useFeedback();

const activeTab = ref('overview');
const syncState = ref<SyncState | null>(null);
const syncingGames = ref<Set<string>>(new Set());
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
    case 'Disabled':
      break;
    default:
      showError({ message: $t('sync_settings.unknown_backend') });
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

const hasEnabledGames = computed(() =>
  (config.value?.games ?? []).some((game) => game.cloud_sync_enabled !== false)
);

interface GameRow {
  name: string;
  isConfig: boolean;
  cloudSyncEnabled: boolean;
  status: 'synced' | 'pending' | 'failed' | 'disabled' | 'conflict' | 'unknown';
  lastSyncAt: string | null;
}

/** Returns true when the error string indicates the bucket requires virtual-hosted-style addressing. */
function isVirtualHostStyleError(msg: string): boolean {
  return /virtual.host/i.test(msg);
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

const gameRows = computed<GameRow[]>(() => {
  const states = syncState.value?.games ?? {};
  const configState = syncState.value?.config_state;
  const configRow: GameRow = {
    name: $t('sync_settings.overview.config_row'),
    isConfig: true,
    cloudSyncEnabled: true,
    status: resolveStatus(savedBackendEnabled.value, configState),
    lastSyncAt: configState?.last_sync_at ?? null,
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
      showError({ message: `${$t('sync_settings.sync_failed')}: ${result.error}` });
      error(`Sync game error for ${gameName}: ${result.error}`);
      if (isVirtualHostStyleError(result.error)) {
        showWarning({ message: $t('sync_settings.s3.virtual_host_hint') });
      }
      return;
    }

    if (result.data === 'conflict') {
      showWarning({ message: statusLabel('conflict') });
    } else {
      showSuccess({ message: $t('sync_settings.sync_success') });
    }
  } catch (e) {
    error(`Sync game exception for ${gameName}: ${e}`);
    showError({ message: String(e) });
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
  if (type === 'Disabled') {
    return { type: 'Disabled' } as Backend;
  }
  return null;
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
  showInfo({ message: $t('sync_settings.start_test') });
  await withLoading(async () => {
    const session = currentSessionConfig();
    if (!session || session.backend.type === 'Disabled') {
      showError({ message: $t('sync_settings.test_failed') });
      return;
    }
    const result = await commands.checkCloudBackend(session);
    if (result.status === 'error') {
      showError({ message: $t('sync_settings.test_failed') });
      error(`${session.backend.type} test error: ${result.error}`);
      if (isVirtualHostStyleError(result.error)) {
        showWarning({ message: $t('sync_settings.s3.virtual_host_hint') });
      }
    } else {
      showSuccess({ message: $t('sync_settings.test_success') });
    }
  }, $t('sync_settings.checking_backend'));
}

async function save() {
  const backend = currentBackend();
  if (!backend) {
    showError({ message: $t('sync_settings.unknown_backend') });
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
  showSuccess({ message: $t('sync_settings.submit_success') });
}

async function abort_change() {
  const loaded = await load_config();
  if (loaded) {
    showSuccess({ message: $t('sync_settings.reset_success') });
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
      showError({ message: $t('sync_settings.upload_failed') });
      return;
    }

    const result = await withLoading(async () => {
      return await commands.cloudUploadAll(session);
    }, $t('sync_settings.uploading_all'));

    if (result.status === 'error') {
      showError({ message: $t('sync_settings.upload_failed') });
      error(`Upload error: ${result.error}`);
      if (isVirtualHostStyleError(result.error)) {
        showWarning({ message: $t('sync_settings.s3.virtual_host_hint') });
      }
    } else if (reportHasFailures(result.data as BatchSyncReportLike)) {
      showError({ message: $t('sync_settings.upload_failed') });
    } else if (reportWasCancelled(result.data as BatchSyncReportLike)) {
      showInfo({ message: $t('sync_settings.canceled') });
    } else {
      showSuccess({ message: $t('sync_settings.upload_success') });
    }
  } catch {
    showInfo({ message: $t('sync_settings.canceled') });
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
      showError({ message: $t('sync_settings.download_failed') });
      return;
    }

    const result = await withLoading(async () => {
      return await commands.cloudDownloadAll(session);
    }, $t('sync_settings.downloading_all'));

    if (result.status === 'error') {
      showError({ message: $t('sync_settings.download_failed') });
      error(`Download error: ${result.error}`);
      if (isVirtualHostStyleError(result.error)) {
        showWarning({ message: $t('sync_settings.s3.virtual_host_hint') });
      }
    } else if (reportHasFailures(result.data as BatchSyncReportLike)) {
      showError({ message: $t('sync_settings.download_failed') });
    } else if (reportWasCancelled(result.data as BatchSyncReportLike)) {
      showInfo({ message: $t('sync_settings.canceled') });
    } else {
      showSuccess({ message: $t('sync_settings.download_success') });
      await load_config();
    }
  } catch {
    showInfo({ message: $t('sync_settings.canceled') });
  } finally {
    await loadSyncState();
  }
}

async function cancelSync() {
  const result = await commands.cancelCloudSync();
  if (result.status === 'error') {
    showError({ message: result.error });
    error(`Cancel sync error: ${result.error}`);
    return;
  }

  if (result.data === 'cancelled') {
    showInfo({ message: $t('cloud_sync.cancelled') });
  }
}

async function open_manual() {
  const result = await commands.openUrl('https://help.sworld.club/docs/extras/cloud');
  if (result.status === 'error') {
    error(`open manual error: ${result.error}`);
    showError({ message: $t('error.open_url_failed') });
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
        <div class="overview-toolbar">
          <ElButton
            type="primary"
            :icon="Refresh"
            :disabled="!savedBackendEnabled || !hasEnabledGames"
            @click="syncAllGames"
          >
            {{ $t('sync_settings.overview.sync_all') }}
          </ElButton>
        </div>

        <ElTable
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
                <span class="game-name">{{ row.name }}</span>
                <ElTag v-if="row.isConfig" size="small" type="info" round effect="plain">
                  <ElIcon :size="12" style="margin-right: 2px"><Lock /></ElIcon>
                  {{ $t('sync_settings.overview.always_synced') }}
                </ElTag>
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
                @change="toggleGameSync(row)"
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
          <ElTableColumn :label="$t('sync_settings.overview.actions')" width="100" align="center">
            <template #default="{ row }">
              <ElButton
                v-if="!row.isConfig && row.cloudSyncEnabled && savedBackendEnabled"
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
        <ElForm label-position="left" :label-width="160" class="backend-form">
          <ElFormItem :label="$t('sync_settings.backend')">
            <ElSelect
              v-model="cloud_settings!.backend!.type"
              :placeholder="$t('sync_settings.backend')"
            >
              <ElOption v-for="b in backends" :key="b" :label="b" :value="b" />
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
                <ElOption value="PathStyle" :label="$t('sync_settings.s3.addressing_style_path')" />
                <ElOption
                  value="VirtualHostedStyle"
                  :label="$t('sync_settings.s3.addressing_style_virtual')"
                />
                <ElOption value="Auto" :label="$t('sync_settings.s3.addressing_style_auto')" />
              </ElSelect>
            </ElFormItem>
          </template>

          <ElFormItem :label="$t('sync_settings.cloud_root')">
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
                @click="check"
              >
                {{ $t('sync_settings.test_button') }}
              </ElButton>
            </div>
          </ElFormItem>
        </ElForm>
      </ElTabPane>

      <!-- Tab 3: Operations -->
      <ElTabPane :label="$t('sync_settings.operations.tab')" name="operations">
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
              :disabled="currentSessionConfig()?.backend.type === 'Disabled'"
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
              :disabled="currentSessionConfig()?.backend.type === 'Disabled'"
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

.game-name {
  font-weight: 500;
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
.backend-form {
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
