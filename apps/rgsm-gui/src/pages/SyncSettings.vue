<script setup lang="ts">
import { ref, computed, onMounted, watch, type Ref } from 'vue';
import { $t } from '../i18n';
import {
  commands,
  type Backend,
  type CloudBackendCheckReport,
  type CloudLibraryStatus,
  type CloudNamespaceGeneration,
} from '../api/commands';
import { error } from '../utils/logger';
import { Cable, ExternalLink, Eye, EyeOff, Layers } from '@lucide/vue';
import { KAlert, KButton, KInput, KNumberInput, KSelect } from '../ui/kit';
import CloudLibraryCutoverDialog from '../components/CloudLibraryCutoverDialog.vue';
import CloudLibraryJoinDialog from '../components/CloudLibraryJoinDialog.vue';
import CloudLibrarySetup from '../components/CloudLibrarySetup.vue';
import CloudLibraryUpgradeCard from '../components/CloudLibraryUpgradeCard.vue';
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
  create(): Promise<CloudLibraryStatus | null>;
}

interface EditableCloudSettings {
  auto_sync_interval: number;
  root_path: string;
  backend: Backend;
  max_concurrency: number;
}

const showWebdavPassword = ref(false);
const showS3Secret = ref(false);

const maxConcurrency = computed({
  get: () => cloud_settings.value.max_concurrency,
  set: (value: number | undefined) => {
    cloud_settings.value.max_concurrency = Math.max(1, value ?? 1);
  },
});

const navItems = computed(() => [
  { key: 'overview' as const, icon: Layers, label: $t('sync_settings.overview.tab') },
  { key: 'backend' as const, icon: Cable, label: $t('sync_settings.console.connection_tab') },
]);

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

const feedback = useFeedback();

const activeTab = ref<'overview' | 'backend'>('overview');
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

function finishLibraryAction(gameCount: number) {
  updateCloudLibraryStatus({ kind: 'active', game_count: gameCount });
}

/** Returns true when the error string indicates the bucket requires virtual-hosted-style addressing. */
function isVirtualHostStyleError(msg: string): boolean {
  return /virtual.host/i.test(msg);
}

function reportHasVirtualHostStyleError(report: CloudBackendCheckReport): boolean {
  return report.items.some((item) => item.message && isVirtualHostStyleError(item.message));
}

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

watch(savedBackendEnabled, (enabled) => {
  if (enabled) void cloudLibrarySetup.value?.inspect();
});

onMounted(async () => {
  try {
    await load_config();
    await loadCloudNamespaceGeneration();
    updatingCloudSettings.value = false;
    void cloudLibrarySetup.value?.inspect();
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
            :aria-current="activeTab === item.key ? 'page' : undefined"
            @click="activeTab = item.key"
          >
            <component :is="item.icon" :size="14" aria-hidden="true" />
            {{ item.label }}
          </button>
        </nav>

        <BackendCheckResult
          v-if="activeTab === 'backend'"
          class="mt-5 border-t border-border px-2 pt-4"
          :report="backendCheckReport"
          :error="backendCheckError"
          :checking="checkingBackend"
          :disabled="currentSessionConfig()?.backend.type === 'Disabled'"
          @test="check"
        />
      </aside>

      <div class="min-w-0 max-w-[820px] flex-1 pb-16">
        <!-- 概览 -->
        <div v-if="activeTab === 'overview'" class="flex flex-col gap-4">
          <CloudSyncOverview v-if="v2LibraryActive" />
          <template v-else>
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
              :title="
                $t('sync_settings.library.join.card', { count: cloudLibraryStatus.game_count })
              "
              :hint="$t('sync_settings.library.join.card_hint')"
              :action="$t('sync_settings.library.join.action')"
              @action="joiningLibrary = true"
            />
            <CloudLibraryUpgradeCard
              v-else-if="cloudLibraryStatus?.kind === 'empty'"
              :kicker="$t('sync_settings.library.title')"
              :title="$t('sync_settings.library.empty')"
              :hint="$t('sync_settings.library.description')"
              :action="$t('sync_settings.library.create')"
              @action="void cloudLibrarySetup?.create()"
            />
            <CloudLibraryUpgradeCard
              v-else
              :kicker="$t('sync_settings.library.title')"
              :title="$t('sync_settings.library.not_checked')"
              :hint="$t('sync_settings.library.description')"
              :action="$t('sync_settings.library.inspect')"
              @action="void cloudLibrarySetup?.inspect()"
            />
          </template>
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
                    :aria-label="showWebdavPassword ? $t('common.hide') : $t('common.show')"
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
                  <p class="mt-1 text-xs text-text-dim">
                    {{ $t('sync_settings.s3.region_hint') }}
                  </p>
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
                    :aria-label="showS3Secret ? $t('common.hide') : $t('common.show')"
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
                <p class="mt-1 text-xs text-text-dim">
                  {{ $t('sync_settings.cloud_root_hint') }}
                </p>
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
        </div>
        <CloudLibrarySetup
          ref="cloudLibrarySetup"
          :enabled="savedBackendEnabled"
          :dirty="hasUnsavedCloudSettings || updatingCloudSettings"
          @status="updateCloudLibraryStatus"
          @busy="cloudLibraryBusy = $event"
        />
      </div>
    </div>

    <CloudLibraryCutoverDialog
      v-model="cuttingOver"
      :resumable="cutoverResumable"
      @cutover="finishLibraryAction"
    />
    <CloudLibraryJoinDialog v-model="joiningLibrary" @joined="finishLibraryAction" />
  </div>
</template>
