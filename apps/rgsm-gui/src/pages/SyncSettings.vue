<script setup lang="ts">
import { ref, computed, onMounted, watch, type Component, type Ref } from 'vue';
import { $t } from '../i18n';
import {
  commands,
  type Backend,
  type CloudBackendCheckReport,
  type CloudLibraryStatus,
  type CloudNamespaceGeneration,
  type ConflictResolution,
  type GameSyncState,
  type SyncState,
} from '../api/commands';
import { error } from '../utils/logger';
import {
  Cable,
  Download,
  ExternalLink,
  Eye,
  EyeOff,
  Layers,
  Lock,
  RefreshCw,
  TriangleAlert,
  Upload,
} from '@lucide/vue';
import { KAlert, KButton, KInput, KNumberInput, KSelect, KSwitch, KTag } from '../ui/kit';
import CloudLibraryCutoverDialog from '../components/CloudLibraryCutoverDialog.vue';
import CloudLibraryJoinDialog from '../components/CloudLibraryJoinDialog.vue';
import CloudLibrarySetup from '../components/CloudLibrarySetup.vue';
import CloudLibraryUpgradeCard from '../components/CloudLibraryUpgradeCard.vue';
import CloudLibraryUpgradeGate from '../components/CloudLibraryUpgradeGate.vue';
import SyncConflictDialog from '../components/SyncConflictDialog.vue';
import BackendCheckResult from '../components/BackendCheckResult.vue';
import CloudSyncOverview from '../components/CloudSyncOverview.vue';
import { resolveCloudUiMode } from '../utils/cloudNamespace';

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

interface CloudLibraryInspectOptions {
  createWhenEmpty?: boolean;
}

interface CloudLibrarySetupHandle {
  inspect(options?: CloudLibraryInspectOptions): Promise<CloudLibraryStatus | null>;
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

const showWebdavPassword = ref(false);
const showS3Secret = ref(false);

const maxConcurrency = computed({
  get: () => cloud_settings.value.max_concurrency,
  set: (value: number | undefined) => {
    cloud_settings.value.max_concurrency = Math.max(1, value ?? 1);
  },
});

const navItems = computed(() => {
  const items: { key: 'overview' | 'backend' | 'operations'; icon: Component; label: string }[] = [
    { key: 'overview', icon: Layers, label: $t('sync_settings.overview.tab') },
    { key: 'backend', icon: Cable, label: $t('sync_settings.console.connection_tab') },
  ];
  if (!v2LibraryActive.value) {
    items.push({
      key: 'operations',
      icon: TriangleAlert,
      label: $t('sync_settings.console.danger_tab'),
    });
  }
  return items;
});

const backendOptions = computed(() =>
  backends.map((backend) => ({ value: backend.value, label: $t(backend.label) }))
);
const addressingOptions = computed(() => [
  { value: 'PathStyle', label: $t('sync_settings.s3.addressing_style_path') },
  { value: 'VirtualHostedStyle', label: $t('sync_settings.s3.addressing_style_virtual') },
  { value: 'Auto', label: $t('sync_settings.s3.addressing_style_auto') },
]);

const backends = [
  { value: 'WebDAV', label: 'sync_settings.backend_label.webdav' },
  { value: 'S3', label: 'sync_settings.backend_label.s3' },
  { value: 'Fs', label: 'sync_settings.backend_label.fs' },
  { value: 'Disabled', label: 'sync_settings.backend_label.disabled' },
] as const;

const { config, refreshConfig, saveConfig } = useConfig();
const { withLoading } = useGlobalLoading();
const feedback = useFeedback();

const activeTab = ref<'overview' | 'backend' | 'operations'>('overview');
const syncState = ref<SyncState | null>(null);
const syncingGames = ref<Set<string>>(new Set());
const syncingConfig = ref(false);
const resolvingConflict = ref(false);
const conflictDialogVisible = ref(false);
const selectedConflictGameName = ref<string | null>(null);
const checkingBackend = ref(false);
const backendCheckReport = ref<CloudBackendCheckReport | null>(null);
const backendCheckError = ref<string | null>(null);
const backendCheckGeneration = ref(0);
const cloudLibrarySetup = ref<CloudLibrarySetupHandle | null>(null);
const cloudLibraryStatus = ref<CloudLibraryStatus | null>(null);
const cloudNamespaceGeneration = ref<CloudNamespaceGeneration | null>(null);
const cloudLibraryBusy = ref(false);
const cuttingOver = ref(false);
const joiningLibrary = ref(false);
const updatingCloudSettings = ref(true);
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
const cloudUiMode = computed(() => resolveCloudUiMode(cloudNamespaceGeneration.value));
const v2LibraryActive = computed(() => cloudUiMode.value === 'v2');
const cutoverRequired = computed(() => cloudLibraryStatus.value?.kind === 'cutover_required');
const joinRequired = computed(() => cloudLibraryStatus.value?.kind === 'join_required');
const cutoverResumable = computed(
  () => cloudLibraryStatus.value?.kind === 'cutover_required' && cloudLibraryStatus.value.resumable
);
const legacyCloudControlsEnabled = computed(() => cloudUiMode.value === 'legacy');
const overviewBlocked = computed(
  () => (cutoverRequired.value || joinRequired.value) && cloudUiMode.value === 'legacy'
);
const snapshotSyncInterval = computed({
  get: () => cloud_settings.value.auto_sync_interval || 5,
  set: (minutes: number | undefined) => {
    cloud_settings.value.auto_sync_interval = minutes ?? 5;
  },
});
const savedConnectionKey = computed(() =>
  normalizedConnectionKey(config.value?.settings.cloud_settings)
);
const draftConnectionKey = computed(() => {
  const settings = draftCloudSettings(false);
  return settings ? JSON.stringify(settings) : '';
});
const normalizedDraftConnectionKey = computed(() => {
  const settings = draftCloudSettings(false);
  return settings ? normalizedConnectionKey(settings) : '';
});
const hasUnsavedCloudSettings = computed(
  () => normalizedDraftConnectionKey.value !== savedConnectionKey.value
);
const cloudSettingsActionBusy = computed(
  () => updatingCloudSettings.value || cloudLibraryBusy.value
);

function updateCloudLibraryStatus(status: CloudLibraryStatus | null) {
  cloudLibraryStatus.value = status;
  if (status?.kind === 'active') {
    cloudNamespaceGeneration.value = 'v2';
  }
}

function openLibraryAction() {
  activeTab.value = 'backend';
  if (cutoverRequired.value) {
    cuttingOver.value = true;
    return;
  }
  if (joinRequired.value) {
    joiningLibrary.value = true;
  }
}

function finishLibraryAction(gameCount: number) {
  updateCloudLibraryStatus({ kind: 'active', game_count: gameCount });
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

type StatusKey = GameRow['status'];
const STATUS_META: Record<
  StatusKey,
  { tone: 'success' | 'warning' | 'danger' | 'neutral'; labelKey: string }
> = {
  synced: { tone: 'success', labelKey: 'sync_settings.overview.status_synced' },
  pending: { tone: 'warning', labelKey: 'sync_settings.overview.status_pending' },
  failed: { tone: 'danger', labelKey: 'sync_settings.overview.status_failed' },
  disabled: { tone: 'neutral', labelKey: 'sync_settings.overview.status_disabled' },
  conflict: { tone: 'warning', labelKey: 'sync_settings.overview.status_conflict' },
  unknown: { tone: 'neutral', labelKey: 'sync_settings.overview.status_unknown' },
};

function statusLabel(status: StatusKey) {
  return $t(STATUS_META[status]?.labelKey ?? status);
}

function statusTone(status: StatusKey) {
  return STATUS_META[status]?.tone ?? 'neutral';
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

async function retryConfigSync() {
  if (!legacyCloudControlsEnabled.value || !savedBackendEnabled.value || syncingConfig.value)
    return;
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
  if (
    !legacyCloudControlsEnabled.value ||
    !savedBackendEnabled.value ||
    syncingGames.value.has(gameName)
  )
    return;
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
  if (!legacyCloudControlsEnabled.value || !savedBackendEnabled.value) return;
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

watch(draftConnectionKey, (value, previous) => {
  if (value === previous) return;
  backendCheckGeneration.value += 1;
  backendCheckReport.value = null;
  backendCheckError.value = null;
  checkingBackend.value = false;
});

function normalizeEndpoint(endpoint: string): string {
  return endpoint.endsWith('/') ? endpoint.slice(0, -1) : endpoint;
}

function normalizedConnectionKey(settings: Partial<EditableCloudSettings> | undefined): string {
  const normalized = toEditableCloudSettings(settings);
  if (normalized.backend.type === 'WebDAV' || normalized.backend.type === 'S3') {
    normalized.backend.endpoint = normalizeEndpoint(normalized.backend.endpoint);
  }
  return JSON.stringify(normalized);
}

function currentBackend(normalize = true): Backend | null {
  const type = cloud_settings.value?.backend?.type;
  if (type === 'WebDAV') {
    return {
      ...cloneValue(webdav_settings.value),
      endpoint: normalize
        ? normalizeEndpoint(webdav_settings.value.endpoint)
        : webdav_settings.value.endpoint,
    };
  }
  if (type === 'S3') {
    return {
      ...cloneValue(s3_settings.value),
      endpoint: normalize
        ? normalizeEndpoint(s3_settings.value.endpoint)
        : s3_settings.value.endpoint,
    };
  }
  if (type === 'Fs') {
    return { type: 'Fs' } as Backend;
  }
  if (type === 'Disabled') {
    return { type: 'Disabled' } as Backend;
  }
  return null;
}

function draftCloudSettings(normalize = true): EditableCloudSettings | null {
  const backend = currentBackend(normalize);
  if (!backend) return null;
  return {
    ...cloneValue(cloud_settings.value),
    backend,
  };
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
  const requestGeneration = backendCheckGeneration.value + 1;
  backendCheckGeneration.value = requestGeneration;
  backendCheckReport.value = null;
  backendCheckError.value = null;
  checkingBackend.value = true;
  try {
    const session = currentSessionConfig();
    if (!session || session.backend.type === 'Disabled') {
      notifyError($t('sync_settings.test_failed'));
      return;
    }
    const result = await commands.checkCloudBackend(session);
    if (requestGeneration !== backendCheckGeneration.value) return;
    if (result.status === 'error') {
      backendCheckError.value = result.error;
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
    if (requestGeneration === backendCheckGeneration.value) {
      checkingBackend.value = false;
    }
  }
}

async function save() {
  if (cloudSettingsActionBusy.value) return;
  const backend = currentBackend();
  if (!backend) {
    notifyError($t('sync_settings.unknown_backend'));
    return;
  }

  if (cutoverResumable.value && hasUnsavedCloudSettings.value) {
    try {
      await feedback.confirm(
        $t('sync_settings.library.cutover.save_warning'),
        $t('sync_settings.library.cutover.save_warning_title'),
        {
          confirmButtonText: $t('sync_settings.save_button'),
          cancelButtonText: $t('sync_settings.cancel'),
          type: 'warning',
        }
      );
    } catch {
      return;
    }
  }

  const previousCloudSettings = cloneValue(config.value!.settings.cloud_settings);
  let saved = false;
  updatingCloudSettings.value = true;
  try {
    config.value!.settings.cloud_settings = {
      ...cloneValue(cloud_settings.value),
      backend,
    };

    saved = await submit_settings();
    if (!saved) return;
    await loadCloudNamespaceGeneration();
    await cloudLibrarySetup.value?.inspect({
      createWhenEmpty: backend.type !== 'Disabled',
    });
  } finally {
    if (!saved) {
      config.value!.settings.cloud_settings = previousCloudSettings;
    }
    updatingCloudSettings.value = false;
  }
}

async function load_config() {
  const loaded = await refreshConfig();
  if (loaded) {
    loadDraftFromConfig();
  }
  return loaded;
}

async function submit_settings(): Promise<boolean> {
  const saved = await saveConfig();
  if (!saved) {
    error('Failed to set config');
    return false;
  }
  await load_config();
  notifySuccess($t('sync_settings.submit_success'));
  return true;
}

async function abort_change() {
  if (cloudSettingsActionBusy.value) return;
  updatingCloudSettings.value = true;
  try {
    const loaded = await load_config();
    if (loaded) {
      notifySuccess($t('sync_settings.reset_success'));
      await cloudLibrarySetup.value?.inspect();
    }
  } finally {
    updatingCloudSettings.value = false;
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
  if (!legacyCloudControlsEnabled.value) return;
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
  if (!legacyCloudControlsEnabled.value) return;
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
      const loaded = await load_config();
      if (loaded) {
        await loadCloudNamespaceGeneration();
        await cloudLibrarySetup.value?.inspect();
      }
    }
  } catch {
    notifyInfo($t('sync_settings.canceled'), undefined, { silent: true });
  } finally {
    await loadSyncState();
  }
}

async function open_manual() {
  const result = await commands.openUrl('https://help.sworld.club/docs/extras/cloud');
  if (result.status === 'error') {
    error(`open manual error: ${result.error}`);
    notifyError($t('error.open_url_failed'));
  }
}

async function loadCloudNamespaceGeneration() {
  const result = await commands.getCloudNamespaceGeneration();
  if (result.status === 'error') {
    cloudNamespaceGeneration.value = null;
    error(`Failed to load Cloud Library generation: ${result.error}`);
    return;
  }
  cloudNamespaceGeneration.value = result.data;
}

watch(v2LibraryActive, (active) => {
  if (active && activeTab.value === 'operations') {
    activeTab.value = 'overview';
  }
});

onMounted(async () => {
  try {
    await load_config();
    await loadCloudNamespaceGeneration();
    updatingCloudSettings.value = false;
    void cloudLibrarySetup.value?.inspect();
    void loadSyncState();
  } catch {
    updatingCloudSettings.value = false;
  }
});
</script>

<template>
  <div class="h-full overflow-y-auto">
    <div class="mx-auto flex max-w-[1080px] gap-10 px-6 py-6">
      <aside class="sticky top-6 w-44 shrink-0 self-start">
        <div class="mb-4 flex items-center justify-between px-2">
          <h1 class="text-lg font-semibold text-text">{{ $t('sync_settings.title') }}</h1>
          <KButton
            variant="ghost"
            size="sm"
            :aria-label="$t('sync_settings.manual_link')"
            @click="open_manual"
          >
            <template #icon><ExternalLink :size="14" aria-hidden="true" /></template>
          </KButton>
        </div>
        <nav class="flex flex-col gap-0.5" :aria-label="$t('sync_settings.title')">
          <button
            v-for="item in navItems"
            :key="item.key"
            type="button"
            class="flex cursor-pointer items-center gap-2 rounded-sm border-none bg-transparent px-2 py-1.5 text-left text-[13px] transition-colors focus-visible:outline-2 focus-visible:outline-accent"
            :class="
              activeTab === item.key
                ? 'bg-surface-2 font-semibold text-text'
                : 'text-text-dim hover:bg-surface-2/60 hover:text-text'
            "
            @click="activeTab = item.key"
          >
            <component :is="item.icon" :size="14" aria-hidden="true" />
            {{ item.label }}
          </button>
        </nav>
      </aside>

      <div class="min-w-0 max-w-[820px] flex-1 pb-16">
        <!-- 概览 -->
        <div v-if="activeTab === 'overview'" class="flex flex-col gap-4">
          <CloudSyncOverview v-if="v2LibraryActive" />

          <div v-if="legacyCloudControlsEnabled && !overviewBlocked" class="flex gap-2">
            <KButton
              variant="primary"
              :disabled="!savedBackendEnabled || !hasEnabledGames"
              @click="syncAllGames"
            >
              <template #icon><RefreshCw :size="14" aria-hidden="true" /></template>
              {{ $t('sync_settings.overview.sync_all') }}
            </KButton>
          </div>

          <section v-if="legacyCloudControlsEnabled || overviewBlocked">
            <CloudLibraryUpgradeGate
              :blocked="overviewBlocked"
              :title="
                joinRequired
                  ? $t('sync_settings.library.join.gate_title')
                  : $t('sync_settings.library.cutover.gate_title')
              "
              :action="
                joinRequired
                  ? $t('sync_settings.library.join.gate_action')
                  : $t('sync_settings.library.cutover.gate_action')
              "
              @upgrade="openLibraryAction"
            >
              <div class="rounded-md border border-border">
                <div
                  class="grid grid-cols-[minmax(0,1fr)_5.5rem_7rem_10rem_8rem] items-center gap-3 border-b border-border px-3 py-2 text-xs font-medium text-text-dim"
                >
                  <span>{{ $t('sync_settings.overview.game_name') }}</span>
                  <span class="text-center">{{ $t('sync_settings.overview.cloud_sync') }}</span>
                  <span class="text-center">{{ $t('sync_settings.overview.status') }}</span>
                  <span class="text-center">{{ $t('sync_settings.overview.last_sync') }}</span>
                  <span class="text-center">{{ $t('sync_settings.overview.actions') }}</span>
                </div>
                <div
                  v-if="gameRows.length === 0"
                  class="px-3 py-8 text-center text-sm text-text-dim"
                >
                  {{ $t('sync_settings.overview.no_games') }}
                </div>
                <div
                  v-for="row in gameRows"
                  :key="row.name"
                  class="grid grid-cols-[minmax(0,1fr)_5.5rem_7rem_10rem_8rem] items-center gap-3 border-b border-border px-3 py-2.5 last:border-b-0"
                >
                  <div class="min-w-0">
                    <div class="flex items-center gap-1.5">
                      <span class="truncate text-sm font-medium text-text">{{ row.name }}</span>
                      <KTag v-if="row.isConfig">
                        <Lock :size="10" aria-hidden="true" />
                        {{ $t('sync_settings.overview.always_synced') }}
                      </KTag>
                    </div>
                    <div v-if="row.detail" class="mt-0.5 truncate text-xs text-danger">
                      {{ row.detail }}
                    </div>
                  </div>
                  <div class="flex justify-center">
                    <KSwitch
                      v-if="!row.isConfig"
                      v-model="row.cloudSyncEnabled"
                      :disabled="!legacyCloudControlsEnabled"
                      :aria-label="$t('sync_settings.overview.cloud_sync')"
                      @update:model-value="toggleGameSync(row)"
                    />
                    <Lock v-else :size="14" class="text-text-dim" aria-hidden="true" />
                  </div>
                  <div class="flex justify-center">
                    <KTag :tone="statusTone(row.status)">{{ statusLabel(row.status) }}</KTag>
                  </div>
                  <div class="text-center font-mono text-xs text-text-dim">
                    {{ formatTime(row.lastSyncAt) }}
                  </div>
                  <div class="flex justify-end">
                    <KButton
                      v-if="
                        row.isConfig &&
                        row.status === 'failed' &&
                        savedBackendEnabled &&
                        legacyCloudControlsEnabled
                      "
                      variant="ghost"
                      size="sm"
                      :loading="syncingConfig"
                      @click="retryConfigSync"
                    >
                      <template #icon><RefreshCw :size="13" aria-hidden="true" /></template>
                      {{ $t('sync_settings.config_retry') }}
                    </KButton>
                    <KButton
                      v-else-if="
                        legacyCloudControlsEnabled && !row.isConfig && row.status === 'conflict'
                      "
                      variant="ghost"
                      size="sm"
                      class="text-warning"
                      @click="openConflictDialog(row)"
                    >
                      <template #icon><TriangleAlert :size="13" aria-hidden="true" /></template>
                      {{ $t('sync_settings.conflict.resolve') }}
                    </KButton>
                    <KButton
                      v-else-if="
                        legacyCloudControlsEnabled &&
                        !row.isConfig &&
                        row.cloudSyncEnabled &&
                        savedBackendEnabled
                      "
                      variant="ghost"
                      size="sm"
                      :loading="syncingGames.has(row.name)"
                      @click="syncGame(row.name)"
                    >
                      <template #icon><RefreshCw :size="13" aria-hidden="true" /></template>
                      {{ $t('sync_settings.console.sync_now') }}
                    </KButton>
                  </div>
                </div>
              </div>
            </CloudLibraryUpgradeGate>
          </section>
        </div>

        <!-- 连接 -->
        <div v-else-if="activeTab === 'backend'" class="flex flex-col gap-4">
          <CloudLibraryUpgradeCard
            v-if="cutoverRequired && cloudLibraryStatus?.kind === 'cutover_required'"
            :kicker="
              cloudLibraryStatus.resumable
                ? $t('sync_settings.library.cutover.card_resume')
                : $t('sync_settings.library.cutover.card_kicker')
            "
            :title="
              $t('sync_settings.library.cutover.card', { count: cloudLibraryStatus.game_count })
            "
            :hint="$t('sync_settings.library.cutover.card_hint')"
            :action="
              cloudLibraryStatus.resumable
                ? $t('sync_settings.library.cutover.resume_action')
                : $t('sync_settings.library.cutover.action')
            "
            @action="cuttingOver = true"
          />
          <CloudLibraryUpgradeCard
            v-else-if="joinRequired && cloudLibraryStatus?.kind === 'join_required'"
            :kicker="$t('sync_settings.library.join.card_kicker')"
            :title="$t('sync_settings.library.join.card', { count: cloudLibraryStatus.game_count })"
            :hint="$t('sync_settings.library.join.card_hint')"
            :action="$t('sync_settings.library.join.action')"
            @action="joiningLibrary = true"
          />
          <KAlert tone="warning">{{ $t('sync_settings.warning') }}</KAlert>

          <div class="flex flex-col">
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="w-40 shrink-0 text-sm text-text">{{ $t('sync_settings.backend') }}</span>
              <KSelect
                v-model="cloud_settings!.backend!.type"
                class="w-56"
                :options="backendOptions"
                :placeholder="$t('sync_settings.backend')"
                :aria-label="$t('sync_settings.backend')"
              />
            </div>

            <template v-if="cloud_settings!.backend!.type === 'WebDAV'">
              <div class="flex items-center justify-between gap-4 py-1.5">
                <span class="w-40 shrink-0 text-sm text-text">{{
                  $t('sync_settings.webdav.endpoint')
                }}</span>
                <KInput v-model="webdav_settings.endpoint" class="w-full" mono />
              </div>
              <div class="flex items-center justify-between gap-4 py-1.5">
                <span class="w-40 shrink-0 text-sm text-text">{{
                  $t('sync_settings.webdav.username')
                }}</span>
                <KInput v-model="webdav_settings.username" class="w-full" />
              </div>
              <div class="flex items-center justify-between gap-4 py-1.5">
                <span class="w-40 shrink-0 text-sm text-text">{{
                  $t('sync_settings.webdav.password')
                }}</span>
                <div class="flex min-w-0 flex-1 items-center gap-1">
                  <KInput
                    v-model="webdav_settings.password"
                    class="w-full"
                    :type="showWebdavPassword ? 'text' : 'password'"
                    mono
                  />
                  <KButton
                    variant="ghost"
                    size="sm"
                    :aria-label="showWebdavPassword ? 'hide' : 'show'"
                    @click="showWebdavPassword = !showWebdavPassword"
                  >
                    <template #icon>
                      <EyeOff v-if="showWebdavPassword" :size="14" aria-hidden="true" />
                      <Eye v-else :size="14" aria-hidden="true" />
                    </template>
                  </KButton>
                </div>
              </div>
            </template>

            <template v-if="cloud_settings!.backend!.type === 'S3'">
              <div class="flex items-center justify-between gap-4 py-1.5">
                <span class="w-40 shrink-0 text-sm text-text">{{
                  $t('sync_settings.s3.endpoint')
                }}</span>
                <KInput v-model="s3_settings.endpoint" class="w-full" mono />
              </div>
              <div class="flex items-center justify-between gap-4 py-1.5">
                <span class="w-40 shrink-0 text-sm text-text">{{
                  $t('sync_settings.s3.bucket')
                }}</span>
                <KInput v-model="s3_settings.bucket" class="w-full" mono />
              </div>
              <div class="flex items-center justify-between gap-4 py-1.5">
                <span class="w-40 shrink-0 text-sm text-text">{{
                  $t('sync_settings.s3.region')
                }}</span>
                <div class="min-w-0 flex-1">
                  <KInput v-model="s3_settings.region" class="w-full" mono />
                  <p class="mt-1 text-xs text-text-dim">{{ $t('sync_settings.s3.region_hint') }}</p>
                </div>
              </div>
              <div class="flex items-center justify-between gap-4 py-1.5">
                <span class="w-40 shrink-0 text-sm text-text">{{
                  $t('sync_settings.s3.access_key_id')
                }}</span>
                <KInput v-model="s3_settings.access_key_id" class="w-full" mono />
              </div>
              <div class="flex items-center justify-between gap-4 py-1.5">
                <span class="w-40 shrink-0 text-sm text-text">{{
                  $t('sync_settings.s3.secret_access_key')
                }}</span>
                <div class="flex min-w-0 flex-1 items-center gap-1">
                  <KInput
                    v-model="s3_settings.secret_access_key"
                    class="w-full"
                    :type="showS3Secret ? 'text' : 'password'"
                    mono
                  />
                  <KButton
                    variant="ghost"
                    size="sm"
                    :aria-label="showS3Secret ? 'hide' : 'show'"
                    @click="showS3Secret = !showS3Secret"
                  >
                    <template #icon>
                      <EyeOff v-if="showS3Secret" :size="14" aria-hidden="true" />
                      <Eye v-else :size="14" aria-hidden="true" />
                    </template>
                  </KButton>
                </div>
              </div>
              <div class="flex items-center justify-between gap-4 py-1.5">
                <span class="w-40 shrink-0 text-sm text-text">{{
                  $t('sync_settings.s3.addressing_style')
                }}</span>
                <KSelect
                  v-model="s3_settings.addressing_style"
                  class="w-56"
                  :options="addressingOptions"
                  :aria-label="$t('sync_settings.s3.addressing_style')"
                />
              </div>
            </template>

            <div
              v-if="cloud_settings!.backend!.type === 'Fs'"
              class="flex items-center justify-between gap-4 py-1.5"
            >
              <span class="w-40 shrink-0 text-sm text-text">{{ $t('sync_settings.fs.root') }}</span>
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-1">
                  <KInput v-model="cloud_settings!.root_path" class="w-full" mono />
                  <KButton variant="ghost" size="sm" @click="chooseFsRoot">
                    {{ $t('sync_settings.fs.choose') }}
                  </KButton>
                </div>
                <p class="mt-1 text-xs text-text-dim">{{ $t('sync_settings.fs.root_hint') }}</p>
              </div>
            </div>
            <div v-else class="flex items-center justify-between gap-4 py-1.5">
              <span class="w-40 shrink-0 text-sm text-text">{{
                $t('sync_settings.cloud_root')
              }}</span>
              <div class="min-w-0 flex-1">
                <KInput v-model="cloud_settings!.root_path" class="w-full" mono />
                <p class="mt-1 text-xs text-text-dim">{{ $t('sync_settings.cloud_root_hint') }}</p>
              </div>
            </div>
            <div class="flex items-center justify-between gap-4 py-1.5">
              <span class="w-40 shrink-0 text-sm text-text">{{
                $t('sync_settings.max_concurrency')
              }}</span>
              <div class="min-w-0 flex-1">
                <KNumberInput v-model="maxConcurrency" :min="1" :max="32" class="w-28" />
                <p class="mt-1 text-xs text-text-dim">
                  {{ $t('sync_settings.max_concurrency_hint') }}
                </p>
              </div>
            </div>
            <div v-if="v2LibraryActive" class="flex items-center justify-between gap-4 py-1.5">
              <span class="w-40 shrink-0 text-sm text-text">{{
                $t('sync_settings.auto_sync_interval')
              }}</span>
              <KNumberInput v-model="snapshotSyncInterval" :min="1" :max="1440" class="w-28" />
            </div>

            <div class="mt-3 flex gap-2 border-t border-border pt-4">
              <KButton variant="primary" :disabled="cloudSettingsActionBusy" @click="save">
                {{ $t('sync_settings.save_button') }}
              </KButton>
              <KButton :disabled="cloudSettingsActionBusy" @click="abort_change">
                {{ $t('sync_settings.abort_button') }}
              </KButton>
            </div>
          </div>

          <CloudLibrarySetup
            ref="cloudLibrarySetup"
            :enabled="savedBackendEnabled"
            :dirty="hasUnsavedCloudSettings || updatingCloudSettings"
            @status="updateCloudLibraryStatus"
            @busy="cloudLibraryBusy = $event"
          />
          <BackendCheckResult
            :report="backendCheckReport"
            :error="backendCheckError"
            :checking="checkingBackend"
            :disabled="currentSessionConfig()?.backend.type === 'Disabled'"
            @test="check"
          />
        </div>

        <!-- 危险操作(v1) -->
        <div v-else-if="activeTab === 'operations'" class="flex flex-col gap-4">
          <KAlert tone="warning">{{ $t('sync_settings.operations.warning') }}</KAlert>
          <div class="flex items-center justify-between gap-4 border-b border-border pb-4">
            <div class="min-w-0">
              <h4 class="text-sm font-medium text-text">
                {{ $t('sync_settings.overwrite_upload') }}
              </h4>
              <p class="mt-0.5 text-xs leading-relaxed text-text-dim">
                {{ $t('sync_settings.operations.upload_desc') }}
              </p>
            </div>
            <KButton
              variant="danger"
              :disabled="
                !legacyCloudControlsEnabled || currentSessionConfig()?.backend.type === 'Disabled'
              "
              @click="upload_all"
            >
              <template #icon><Upload :size="14" aria-hidden="true" /></template>
              {{ $t('sync_settings.overwrite_upload') }}
            </KButton>
          </div>
          <div class="flex items-center justify-between gap-4">
            <div class="min-w-0">
              <h4 class="text-sm font-medium text-text">
                {{ $t('sync_settings.overwrite_download') }}
              </h4>
              <p class="mt-0.5 text-xs leading-relaxed text-text-dim">
                {{ $t('sync_settings.operations.download_desc') }}
              </p>
            </div>
            <KButton
              variant="danger"
              :disabled="
                !legacyCloudControlsEnabled || currentSessionConfig()?.backend.type === 'Disabled'
              "
              @click="download_all"
            >
              <template #icon><Download :size="14" aria-hidden="true" /></template>
              {{ $t('sync_settings.overwrite_download') }}
            </KButton>
          </div>
        </div>
      </div>
    </div>

    <CloudLibraryCutoverDialog
      v-model="cuttingOver"
      :resumable="cutoverResumable"
      @cutover="finishLibraryAction"
    />
    <CloudLibraryJoinDialog v-model="joiningLibrary" @joined="finishLibraryAction" />
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
