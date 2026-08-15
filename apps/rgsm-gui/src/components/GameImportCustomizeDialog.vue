<template>
  <KDialog
    v-model:open="dialogVisible"
    :title="$t('game_import_customize.title', { name: gameName })"
    :width="920"
    :dismissable="false"
  >
    <div
      class="relative flex max-h-[70vh] min-h-80 flex-col gap-4 overflow-x-hidden overflow-y-auto"
    >
      <!-- Game name input -->
      <div>
        <label class="mb-1 block text-xs text-text-dim">{{ $t('addgame.game_name') }}</label>
        <KInput v-model="form.gameName" :placeholder="$t('addgame.input_game_name_prompt')" />
      </div>

      <!-- Steam User ID selector -->
      <div>
        <label class="mb-1 block text-xs text-text-dim">{{
          $t('game_batch_import.store_user_id')
        }}</label>
        <div class="flex items-center gap-2">
          <div class="w-72">
            <KInput
              v-model="storeUserIdInput"
              :placeholder="$t('game_batch_import.store_user_id_placeholder')"
              aria-label="Steam user ID"
              list="customize-user-id-candidates"
              mono
              @blur="handleStoreUserIdChange"
            />
            <datalist id="customize-user-id-candidates">
              <option
                v-for="c in userIdCandidates"
                :key="c.userId"
                :value="c.userId"
                :label="formatUserIdLabel(c)"
              />
            </datalist>
          </div>
          <span class="text-xs text-text-dim">{{
            $t('game_batch_import.store_user_id_hint')
          }}</span>
        </div>
      </div>

      <!-- Save paths -->
      <div>
        <div class="mb-2 flex items-center justify-between gap-2">
          <label class="text-xs text-text-dim">{{ $t('game_import_customize.save_paths') }}</label>
          <div class="flex items-center gap-1.5">
            <KButton size="sm" :disabled="isChecking" @click="checkAllPaths()">
              <LoaderCircle v-if="isChecking" :size="12" class="animate-spin" aria-hidden="true" />
              {{ $t('game_import_customize.verify_paths') }}
            </KButton>
            <KButton size="sm" @click="selectAllSupported">
              {{ $t('game_import_customize.select_all_supported') }}
            </KButton>
            <KButton size="sm" variant="primary" @click="selectByCheck">
              {{ $t('game_import_customize.select_by_check') }}
            </KButton>
          </div>
        </div>

        <div class="rounded-sm border border-border">
          <div
            v-for="(row, index) in form.savePaths"
            :key="index"
            class="flex items-start gap-2 border-b border-border px-3 py-2 last:border-b-0"
          >
            <KCheckbox
              :model-value="isRowSelected(row)"
              :aria-label="row.path"
              class="mt-2"
              @update:model-value="toggleRow(row)"
            />
            <div class="flex min-w-0 flex-1 items-start gap-2">
              <PathVariableInput
                v-model="row.path"
                class="min-w-0 flex-1"
                status-mode="below"
                :store-user-id="selectedStoreUserId"
                :install-dirs="props.installDirs"
                :steam-id="props.steamId"
              />
              <KTag v-if="isRegistryPath(row.path)" class="mt-1.5 shrink-0">
                {{ $t('game_import_customize.registry') }}
              </KTag>
            </div>
            <KTag
              class="mt-1.5 w-16 shrink-0 justify-center"
              :tone="getPathKindTagTone(row, index)"
            >
              {{ getPathKindLabel(row, index) }}
            </KTag>
            <div class="mt-1.5 flex w-24 shrink-0 flex-wrap gap-1">
              <KTag v-for="tag in row.tags" :key="tag">{{ tag }}</KTag>
            </div>
            <div class="mt-1.5 w-20 shrink-0">
              <template v-if="pathChecks[index]">
                <KTooltip v-if="pathChecks[index]?.error" :content="pathChecks[index]!.error!">
                  <KTag tone="danger">{{ $t('game_import_customize.status_error') }}</KTag>
                </KTooltip>
                <KTag v-else :tone="pathChecks[index]?.exists ? 'success' : 'warning'">
                  {{
                    pathChecks[index]?.exists
                      ? $t('game_import_customize.status_exists')
                      : $t('game_import_customize.status_missing')
                  }}
                </KTag>
              </template>
              <KTag v-else>{{ $t('game_import_customize.status_unchecked') }}</KTag>
            </div>
          </div>
          <div
            v-if="form.savePaths.length === 0"
            class="px-3 py-6 text-center text-sm text-text-dim"
          >
            {{ $t('common.no_data') }}
          </div>
        </div>
      </div>

      <KAlert tone="info">{{ $t('game_import_customize.path_hint') }}</KAlert>

      <div
        v-if="loading"
        class="absolute inset-0 flex items-center justify-center gap-2 bg-surface/70 text-sm text-text-dim"
      >
        <LoaderCircle :size="16" class="animate-spin" aria-hidden="true" />
        {{ $t('common.operation_in_progress') }}
      </div>
    </div>

    <template #footer>
      <KButton @click="handleCancel">{{ $t('common.cancel') }}</KButton>
      <KButton variant="primary" @click="handleConfirm">{{ $t('common.confirm') }}</KButton>
    </template>
  </KDialog>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue';
import { LoaderCircle } from '@lucide/vue';
import { $t } from '../i18n';
import {
  commands,
  type SavePath,
  type PathCheckResult,
  type StoreUserIdCandidate,
} from '../api/commands';
import { error } from '../utils/logger';
import PathVariableInput from './PathVariableInput.vue';
import { KAlert, KButton, KCheckbox, KDialog, KInput, KTag, KTooltip } from '../ui/kit';

interface CustomizeData {
  gameName: string;
  savePaths: SavePath[];
  storeUserId: string | null;
}

const props = defineProps({
  modelValue: {
    type: Boolean,
    required: true,
  },
  gameName: {
    type: String,
    required: true,
  },
  savePaths: {
    type: Array as () => SavePath[],
    default: () => [],
  },
  installDirs: {
    type: Array as () => string[],
    default: () => [],
  },
  steamId: {
    type: Number as () => number | null,
    default: null,
  },
  loading: {
    type: Boolean,
    default: false,
  },
});

const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'confirm', data: CustomizeData): void;
}>();

const dialogVisible = computed({
  get: () => props.modelValue,
  set: (value: boolean) => emit('update:modelValue', value),
});

type PathKind = 'file' | 'folder' | 'registry' | 'unknown';
type PathCheckState = {
  resolvedPath?: string;
  exists?: boolean;
  error?: string;
  kind?: PathKind;
};

const selectedPaths = ref<SavePath[]>([]);
const isChecking = ref(false);
const pathChecks = ref<Array<PathCheckState | null>>([]);

const form = ref<CustomizeData>({
  gameName: props.gameName,
  savePaths: [],
  storeUserId: null,
});

// Store user ID selection (datalist: 候选补全 + 允许任意输入)
const selectedStoreUserId = ref<string | null>(null);
const userIdCandidates = ref<StoreUserIdCandidate[]>([]);
const loadingUserIds = ref(false);

const storeUserIdInput = computed({
  get: () => selectedStoreUserId.value ?? '',
  set: (value: string) => {
    selectedStoreUserId.value = value.trim() === '' ? null : value.trim();
  },
});

async function loadUserIdCandidates() {
  loadingUserIds.value = true;
  try {
    const result = await commands.detectStoreUserIds();
    if (result.status === 'ok') {
      userIdCandidates.value = result.data;
      if (result.data.length > 0 && !selectedStoreUserId.value) {
        selectedStoreUserId.value = result.data[0]!.userId;
      }
    }
  } catch (e) {
    error(`Error detecting store user IDs: ${e}`);
  } finally {
    loadingUserIds.value = false;
  }
}

function formatUserIdLabel(c: StoreUserIdCandidate): string {
  const displayName = c.personaName || c.accountName;
  const identity = displayName ? `${displayName} (${c.userId})` : c.userId;
  if (c.lastModifiedEpochSecs == null) return identity;
  const ago = formatTimeAgo(c.lastModifiedEpochSecs);
  return `${identity} · ${ago}`;
}

function formatTimeAgo(epochSecs: number): string {
  const diffMs = Math.max(0, Date.now() - epochSecs * 1000);
  const mins = Math.floor(diffMs / 60000);
  if (mins < 60) return $t('common.minutes_ago', { n: mins });
  const hours = Math.floor(mins / 60);
  if (hours < 24) return $t('common.hours_ago', { n: hours });
  const days = Math.floor(hours / 24);
  return $t('common.days_ago', { n: days });
}

function isRegistryPath(path: string) {
  return path.startsWith('REGISTRY:') || path.startsWith('HKEY_');
}

function getPathKind(row: SavePath, index: number): PathKind {
  if (isRegistryPath(row.path)) return 'registry';
  return pathChecks.value[index]?.kind ?? 'unknown';
}

function getPathKindLabel(row: SavePath, index: number): string {
  switch (getPathKind(row, index)) {
    case 'file':
      return $t('save_location_drawer.type_file');
    case 'folder':
      return $t('save_location_drawer.type_folder');
    case 'registry':
      return $t('save_location_drawer.type_registry');
    default:
      return $t('game_import_customize.type_unknown');
  }
}

function getPathKindTagTone(row: SavePath, index: number): 'success' | 'warning' | 'neutral' {
  switch (getPathKind(row, index)) {
    case 'file':
      return 'warning';
    case 'folder':
      return 'success';
    default:
      return 'neutral';
  }
}

// 行选择（对象引用，form.savePaths 在 watch 中整体克隆，引用稳定）
function isRowSelected(row: SavePath) {
  return selectedPaths.value.includes(row);
}

function toggleRow(row: SavePath) {
  if (isRowSelected(row)) {
    selectedPaths.value = selectedPaths.value.filter((item) => item !== row);
  } else {
    selectedPaths.value = [...selectedPaths.value, row];
  }
}

// Watch for props changes to update form
watch(
  () => [props.gameName, props.savePaths],
  () => {
    form.value = {
      gameName: props.gameName,
      savePaths: JSON.parse(JSON.stringify(props.savePaths)),
      storeUserId: null,
    };
    pathChecks.value = new Array(form.value.savePaths.length).fill(null);
  },
  { immediate: true, deep: true }
);

watch(
  () => form.value.savePaths.map((item) => item.path),
  () => {
    pathChecks.value = new Array(form.value.savePaths.length).fill(null);
  },
  { deep: true }
);

// Default-select all supported rows when dialog opens
watch(
  () => dialogVisible.value,
  async (open) => {
    if (!open) {
      selectedPaths.value = [];
      return;
    }

    selectedStoreUserId.value = null;
    await loadUserIdCandidates();
    await nextTick();
    await checkAllPaths(true);
  }
);

async function handleStoreUserIdChange() {
  if (!dialogVisible.value || !form.value.savePaths.length) {
    return;
  }
  await checkAllPaths();
}

function handleCancel() {
  emit('update:modelValue', false);
}

function handleConfirm() {
  emit('confirm', {
    gameName: form.value.gameName,
    savePaths: selectedPaths.value,
    storeUserId: selectedStoreUserId.value || null,
  });
  emit('update:modelValue', false);
}

function selectAllSupported() {
  selectedPaths.value = [...form.value.savePaths];
}

function applySelectionByCheck() {
  selectedPaths.value = form.value.savePaths.filter((row, index) => {
    const check = pathChecks.value[index];
    return !!check && !check.error && check.exists === true;
  });
}

async function checkAllPaths(applySelection: boolean = false) {
  if (!form.value.savePaths.length) return;
  isChecking.value = true;
  try {
    const paths = form.value.savePaths.map((p) => p.path);
    const result = await commands.checkPaths(
      paths,
      selectedStoreUserId.value || null,
      props.installDirs.length > 0 ? props.installDirs : null,
      props.steamId
    );
    if (result.status === 'ok') {
      const checks = result.data as PathCheckResult[];
      // Map enum variants to the check object format
      pathChecks.value = checks.map((c) => {
        if (c.status === 'ok') {
          return {
            resolvedPath: c.resolvedPath,
            exists: true,
            kind: c.isFile ? 'file' : 'folder',
          };
        } else if (c.status === 'notFound') {
          return { resolvedPath: c.resolvedPath, exists: false };
        } else if (c.status === 'registryPath') {
          return c.supported
            ? { resolvedPath: c.rawPath, exists: c.exists, kind: 'registry' }
            : {
                error: $t('game_import_customize.registry_not_supported_platform'),
                kind: 'registry',
              };
        } else if (c.status === 'resolveFailed') {
          return { error: c.error };
        }
        return {};
      });
      if (applySelection) {
        await nextTick();
        applySelectionByCheck();
      }
    } else {
      pathChecks.value = form.value.savePaths.map(() => ({ error: result.error }));
      if (applySelection) {
        await nextTick();
        selectAllSupported();
      }
    }
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    pathChecks.value = form.value.savePaths.map(() => ({ error: msg }));
    if (applySelection) {
      await nextTick();
      selectAllSupported();
    }
  } finally {
    isChecking.value = false;
  }
}

async function selectByCheck() {
  const hasAnyCheck = pathChecks.value.some((x) => x !== null);
  if (!hasAnyCheck) {
    await checkAllPaths(true);
    return;
  }
  applySelectionByCheck();
}
</script>
