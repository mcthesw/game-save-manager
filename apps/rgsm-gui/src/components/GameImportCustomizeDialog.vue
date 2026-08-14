<template>
  <el-dialog
    v-model="dialogVisible"
    :title="$t('game_import_customize.title', { name: gameName })"
    width="70%"
    top="5vh"
    :close-on-click-modal="false"
    append-to-body
  >
    <div v-loading="loading" class="customize-dialog-content">
      <!-- Game name input -->
      <el-form :model="form" label-position="top">
        <el-form-item :label="$t('addgame.game_name')">
          <el-input v-model="form.gameName" :placeholder="$t('addgame.input_game_name_prompt')" />
        </el-form-item>

        <!-- Steam User ID selector -->
        <el-form-item :label="$t('game_batch_import.store_user_id')">
          <el-select
            v-model="selectedStoreUserId"
            filterable
            allow-create
            clearable
            :placeholder="$t('game_batch_import.store_user_id_placeholder')"
            class="store-user-id-select"
            :loading="loadingUserIds"
            @change="handleStoreUserIdChange"
          >
            <el-option
              v-for="c in userIdCandidates"
              :key="c.userId"
              :label="formatUserIdLabel(c)"
              :value="c.userId"
            />
          </el-select>
          <span class="store-user-id-hint">{{ $t('game_batch_import.store_user_id_hint') }}</span>
        </el-form-item>

        <!-- Save paths table -->
        <el-form-item :label="$t('game_import_customize.save_paths')">
          <div class="verify-row">
            <el-button size="small" :loading="isChecking" @click="checkAllPaths()">
              {{ $t('game_import_customize.verify_paths') }}
            </el-button>
            <el-button size="small" @click="selectAllSupported">
              {{ $t('game_import_customize.select_all_supported') }}
            </el-button>
            <el-button size="small" type="primary" @click="selectByCheck">
              {{ $t('game_import_customize.select_by_check') }}
            </el-button>
          </div>
          <el-table
            ref="tableRef"
            :data="form.savePaths"
            stripe
            @selection-change="handleSelectionChange"
          >
            <el-table-column type="selection" width="55" :selectable="isRowSelectable" />
            <el-table-column :label="$t('game_import_customize.path')" prop="path" min-width="300">
              <template #default="{ row }">
                <div class="path-cell">
                  <path-variable-input
                    v-model="row.path"
                    class="path-input"
                    status-mode="below"
                    :store-user-id="selectedStoreUserId"
                    :install-dirs="props.installDirs"
                    :steam-id="props.steamId"
                  />
                  <el-tag
                    v-if="isRegistryPath(row.path)"
                    type="info"
                    size="small"
                    class="registry-tag"
                  >
                    {{ $t('game_import_customize.registry') }}
                  </el-tag>
                </div>
              </template>
            </el-table-column>
            <el-table-column :label="$t('game_import_customize.type')" width="110">
              <template #default="{ row, $index }">
                <el-tag :type="getPathKindTagType(tableRowToSavePath(row), $index)" size="small">
                  {{ getPathKindLabel(tableRowToSavePath(row), $index) }}
                </el-tag>
              </template>
            </el-table-column>
            <el-table-column :label="$t('game_import_customize.tags')" width="150">
              <template #default="{ row }">
                <el-tag v-for="tag in row.tags" :key="tag" size="small" class="tag-item">
                  {{ tag }}
                </el-tag>
              </template>
            </el-table-column>
            <el-table-column :label="$t('game_import_customize.resolution')" min-width="220">
              <template #default="{ $index }">
                <div v-if="pathChecks[$index]" class="resolution-cell">
                  <template v-if="pathChecks[$index]?.error">
                    <el-tooltip :content="pathChecks[$index]?.error" placement="top">
                      <el-tag type="danger" size="small">
                        {{ $t('game_import_customize.status_error') }}
                      </el-tag>
                    </el-tooltip>
                  </template>
                  <template v-else>
                    <el-tag :type="pathChecks[$index]?.exists ? 'success' : 'warning'" size="small">
                      {{
                        pathChecks[$index]?.exists
                          ? $t('game_import_customize.status_exists')
                          : $t('game_import_customize.status_missing')
                      }}
                    </el-tag>
                  </template>
                </div>
                <el-tag v-else type="info" size="small">
                  {{ $t('game_import_customize.status_unchecked') }}
                </el-tag>
              </template>
            </el-table-column>
          </el-table>
        </el-form-item>

        <el-alert type="info" :closable="false" class="path-hint">
          {{ $t('game_import_customize.path_hint') }}
        </el-alert>
      </el-form>
    </div>

    <template #footer>
      <div class="dialog-footer">
        <el-button @click="handleCancel">{{ $t('common.cancel') }}</el-button>
        <el-button type="primary" @click="handleConfirm">
          {{ $t('common.confirm') }}
        </el-button>
      </div>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue';
import { $t } from '../i18n';
import {
  commands,
  type SavePath,
  type PathCheckResult,
  type StoreUserIdCandidate,
} from '../api/commands';
import { error } from '../utils/logger';
import PathVariableInput from './PathVariableInput.vue';

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

type ElTableLike = {
  clearSelection?: () => void;
  toggleRowSelection?: (row: SavePath, selected: boolean) => void;
};

type PathKind = 'file' | 'folder' | 'registry' | 'unknown';
type PathCheckState = {
  resolvedPath?: string;
  exists?: boolean;
  error?: string;
  kind?: PathKind;
};

const tableRef = ref<ElTableLike | null>(null);
const selectedPaths = ref<SavePath[]>([]);
const isChecking = ref(false);
const pathChecks = ref<Array<PathCheckState | null>>([]);

const form = ref<CustomizeData>({
  gameName: props.gameName,
  savePaths: [],
  storeUserId: null,
});

// Store user ID selection
const selectedStoreUserId = ref<string | null>(null);
const userIdCandidates = ref<StoreUserIdCandidate[]>([]);
const loadingUserIds = ref(false);

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

function tableRowToSavePath(row: unknown): SavePath {
  return row as SavePath;
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

function getPathKindTagType(row: SavePath, index: number): 'success' | 'warning' | 'info' {
  switch (getPathKind(row, index)) {
    case 'file':
      return 'warning';
    case 'folder':
      return 'success';
    default:
      return 'info';
  }
}

function isRowSelectable(_row: SavePath) {
  // Row selection itself is always enabled; checked/existing-path logic controls effective picks.
  return true;
}

function handleSelectionChange(selection: SavePath[]) {
  selectedPaths.value = selection;
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
  try {
    tableRef.value?.clearSelection?.();
    for (const row of form.value.savePaths) {
      if (isRowSelectable(row)) {
        tableRef.value?.toggleRowSelection?.(row, true);
      }
    }
  } catch {
    // ignore selection errors
  }
}

function applySelectionByCheck() {
  try {
    tableRef.value?.clearSelection?.();
    form.value.savePaths.forEach((row, index) => {
      const check = pathChecks.value[index];
      const selectable = isRowSelectable(row);
      const shouldSelect = selectable && !!check && !check.error && check.exists === true;
      if (selectable) {
        tableRef.value?.toggleRowSelection?.(row, shouldSelect);
      }
    });
  } catch {
    // ignore selection errors
  }
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

<style scoped>
.customize-dialog-content {
  height: 70vh;
  min-height: 320px;
  overflow-y: auto;
}

.store-user-id-select {
  width: 280px;
}

.store-user-id-hint {
  font-size: 12px;
  color: var(--el-text-color-secondary);
  margin-left: 8px;
}

.verify-row {
  display: flex;
  justify-content: flex-end;
  margin-bottom: 8px;
}

.path-cell {
  display: flex;
  align-items: flex-start;
  gap: 8px;
}

.path-input {
  flex: 1;
}

.registry-tag {
  flex-shrink: 0;
  margin-top: 6px;
}

.resolution-cell {
  display: flex;
  align-items: center;
}

.tag-item {
  margin-right: 4px;
}

.path-hint {
  margin-top: 16px;
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
