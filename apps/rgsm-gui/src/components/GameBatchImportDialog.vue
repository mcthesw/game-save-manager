<template>
  <el-dialog
    v-model="dialogVisible"
    :title="$t('game_batch_import.title', { count: games.length })"
    width="80%"
    top="5vh"
    :close-on-click-modal="false"
    append-to-body
  >
    <div v-loading="loading" class="batch-dialog-content">
      <el-alert type="info" :closable="false" class="info-alert">
        {{ $t('game_batch_import.hint') }}
      </el-alert>

      <!-- Steam User ID selector -->
      <div class="store-user-id-row">
        <span class="store-user-id-label">{{ $t('game_batch_import.store_user_id') }}</span>
        <el-select
          v-model="selectedStoreUserId"
          size="small"
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
      </div>

      <div class="toolbar">
        <el-input
          v-model="searchText"
          size="small"
          clearable
          class="search-input"
          :placeholder="$t('game_batch_import.search_placeholder')"
        />
        <el-checkbox v-model="onlySelected" class="only-selected-checkbox">
          {{ $t('game_batch_import.only_selected') }}
        </el-checkbox>
        <el-tag size="small" type="info" class="summary-tag">
          {{
            $t('game_batch_import.selected_summary', {
              games: selectedCount,
              paths: selectedPathsCount,
            })
          }}
        </el-tag>
        <div class="toolbar-actions">
          <el-button size="small" :loading="isChecking" @click="checkAllPaths()">
            {{ $t('game_batch_import.verify_paths') }}
          </el-button>
          <el-button size="small" @click="selectAllSupported">
            {{ $t('game_batch_import.select_all_supported') }}
          </el-button>
          <el-button size="small" type="primary" @click="selectByCheck">
            {{ $t('game_batch_import.select_by_check') }}
          </el-button>
          <el-button size="small" @click="expandAll">
            {{ $t('game_batch_import.expand_all') }}
          </el-button>
          <el-button size="small" @click="collapseAll">
            {{ $t('game_batch_import.collapse_all') }}
          </el-button>
        </div>
      </div>

      <!-- Games list with expand/collapse -->
      <el-collapse v-model="activeGames" class="games-list">
        <el-collapse-item v-for="game in filteredGameConfigs" :key="game.name" :name="game.name">
          <template #title>
            <div class="game-header">
              <el-checkbox v-model="game.selected" class="game-checkbox" @click.stop />
              <span class="game-name">{{ game.name }}</span>
              <el-tag size="small" class="path-count">
                {{ countSelectedPaths(game) }}/{{ game.paths.length }}
                {{ $t('game_batch_import.paths') }}
              </el-tag>
            </div>
          </template>

          <!-- Game name editor -->
          <el-form-item :label="$t('addgame.game_name')">
            <el-input v-model="game.customName" :placeholder="game.name" />
          </el-form-item>

          <!-- Save paths list -->
          <el-form-item :label="$t('game_batch_import.save_paths')">
            <div v-for="(path, pathIndex) in game.paths" :key="pathIndex" class="path-item">
              <el-checkbox v-model="path.selected" class="path-checkbox" />
              <el-input
                v-model="path.path"
                size="small"
                class="path-input"
                :disabled="!path.selected"
              />
              <el-tag v-if="path.isRegistry" type="info" size="small" class="path-tag registry-tag">
                {{ $t('game_batch_import.registry') }}
              </el-tag>
              <template v-else>
                <el-tooltip v-if="path.check?.error" :content="path.check?.error" placement="top">
                  <el-tag type="danger" size="small" class="path-tag">
                    {{ $t('game_batch_import.status_error') }}
                  </el-tag>
                </el-tooltip>
                <el-tooltip
                  v-else-if="path.check?.resolvedPath"
                  :content="path.check?.resolvedPath"
                  placement="top"
                >
                  <el-tag
                    :type="path.check?.exists ? 'success' : 'warning'"
                    size="small"
                    class="path-tag"
                  >
                    {{
                      path.check?.exists
                        ? $t('game_batch_import.status_exists')
                        : $t('game_batch_import.status_missing')
                    }}
                  </el-tag>
                </el-tooltip>
                <el-tag v-else type="info" size="small" class="path-tag">
                  {{ $t('game_batch_import.status_unchecked') }}
                </el-tag>
              </template>
              <el-tag v-for="tag in path.tags" :key="tag" size="small" class="path-tag">
                {{ tag }}
              </el-tag>
            </div>
          </el-form-item>
        </el-collapse-item>
      </el-collapse>
    </div>

    <template #footer>
      <div class="dialog-footer">
        <el-button @click="handleCancel">{{ $t('common.cancel') }}</el-button>
        <el-button type="primary" @click="handleConfirm">
          {{ $t('game_batch_import.import_selected', { count: selectedCount }) }}
        </el-button>
      </div>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { $t } from '../i18n';
import {
  commands,
  type ImportableGame,
  type SavePath,
  type PathCheckResult,
  type StoreUserIdCandidate,
} from '../bindings';
import { error } from '@tauri-apps/plugin-log';
import type { ManifestPathConstraints } from '../bindings';

/** Check if a path is a Windows registry path (not supported for backup) */
function isRegistryPath(path: string): boolean {
  return path.startsWith('REGISTRY:') || path.startsWith('HKEY_');
}

interface GameConfig {
  name: string;
  customName: string;
  installDirs: string[];
  steamId: number | null;
  selected: boolean;
  paths: Array<{
    path: string;
    tags: string[];
    constraints?: ManifestPathConstraints;
    selected: boolean;
    isRegistry: boolean;
    check?: { resolvedPath?: string; exists?: boolean; error?: string } | null;
  }>;
}

const props = defineProps({
  modelValue: {
    type: Boolean,
    required: true,
  },
  games: {
    type: Array as () => ImportableGame[],
    required: true,
  },
  gamePaths: {
    type: Object as () => Record<string, SavePath[]>,
    default: () => ({}),
  },
  loading: {
    type: Boolean,
    default: false,
  },
});

const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'confirm', configs: GameConfig[], storeUserId: string | null): void;
}>();

const dialogVisible = computed({
  get: () => props.modelValue,
  set: (value: boolean) => emit('update:modelValue', value),
});

const activeGames = ref<string[]>([]);

const gameConfigs = ref<GameConfig[]>([]);
const isChecking = ref(false);
const searchText = ref('');
const onlySelected = ref(false);
const autoCheckedOnce = ref(false);

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
  if (c.lastModifiedEpochSecs == null) return c.userId;
  const ago = formatTimeAgo(c.lastModifiedEpochSecs);
  return `${c.userId} (${ago})`;
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

// Initialize game configs when games or paths change
watch(
  () => [props.games, props.gamePaths],
  () => {
    gameConfigs.value = props.games.map((game) => {
      const paths = props.gamePaths[game.name] || [];
      return {
        name: game.name,
        customName: game.name,
        installDirs: game.installDirs ?? [],
        steamId: game.steamId ?? null,
        selected: true,
        paths: paths.map((p) => ({
          path: p.path,
          tags: p.tags,
          constraints: p.constraints,
          isRegistry: isRegistryPath(p.path),
          selected: true,
          check: null,
        })),
      };
    });
  },
  { immediate: true, deep: true }
);

watch(
  () => dialogVisible.value,
  async (open) => {
    if (!open) return;
    autoCheckedOnce.value = false;
    selectedStoreUserId.value = null;
    await loadUserIdCandidates();
  }
);

// Auto-check paths and use results to set default selection after paths finish loading
watch(
  () => [dialogVisible.value, props.loading, loadingUserIds.value],
  async ([open, loading, loadingUserIds]) => {
    if (!open || loading || loadingUserIds) return;
    if (autoCheckedOnce.value) return;
    autoCheckedOnce.value = true;
    await checkAllPaths(true);
  },
  { immediate: true }
);

const selectedCount = computed(() => {
  return gameConfigs.value.filter((g) => g.selected).length;
});

const selectedPathsCount = computed(() => {
  return gameConfigs.value.reduce((acc, game) => {
    if (!game.selected) return acc;
    return acc + game.paths.filter((p) => p.selected).length;
  }, 0);
});

const filteredGameConfigs = computed(() => {
  let list = gameConfigs.value;
  if (onlySelected.value) {
    list = list.filter((g) => g.selected);
  }
  if (searchText.value) {
    const q = searchText.value.toLowerCase();
    list = list.filter(
      (g) => g.name.toLowerCase().includes(q) || g.customName.toLowerCase().includes(q)
    );
  }
  return list;
});

function countSelectedPaths(game: GameConfig) {
  return game.paths.filter((p) => p.selected).length;
}

function expandAll() {
  activeGames.value = filteredGameConfigs.value.map((g) => g.name);
}

function collapseAll() {
  activeGames.value = [];
}

function handleCancel() {
  emit('update:modelValue', false);
}

function handleConfirm() {
  const selected = gameConfigs.value.filter((g) => g.selected);
  emit('confirm', selected, selectedStoreUserId.value || null);
  emit('update:modelValue', false);
}

async function handleStoreUserIdChange() {
  if (!dialogVisible.value || gameConfigs.value.length === 0) {
    return;
  }
  await checkAllPaths();
}

async function checkAllPaths(applySelection: boolean = false) {
  if (gameConfigs.value.length === 0) return;

  isChecking.value = true;
  try {
    for (const game of gameConfigs.value) {
      const paths = game.paths.map((pathItem) => pathItem.path);
      if (paths.length === 0) continue;

      const result = await commands.checkPaths(
        paths,
        selectedStoreUserId.value || null,
        game.installDirs.length > 0 ? game.installDirs : null,
        game.steamId
      );

      if (result.status === 'ok') {
        const checks = result.data as PathCheckResult[];
        checks.forEach((c, pathIndex) => {
          const pathItem = game.paths[pathIndex];
          if (!pathItem) return;

          if (c.status === 'ok') {
            pathItem.check = { resolvedPath: c.resolvedPath, exists: true };
          } else if (c.status === 'notFound') {
            pathItem.check = { resolvedPath: c.resolvedPath, exists: false };
          } else if (c.status === 'registryPath') {
            pathItem.check = c.supported
              ? { resolvedPath: c.rawPath, exists: c.exists }
              : { error: $t('game_batch_import.registry_not_supported_platform') };
          } else if (c.status === 'resolveFailed') {
            pathItem.check = { error: c.error };
          }
        });
      } else {
        game.paths.forEach((pathItem) => {
          pathItem.check = { error: result.error };
        });
      }
    }

    if (applySelection) {
      applySelectionByCheck();
    }
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    gameConfigs.value.forEach((game) => {
      game.paths.forEach((p) => {
        p.check = { error: msg };
      });
    });
    if (applySelection) {
      selectAllSupported();
    }
  } finally {
    isChecking.value = false;
  }
}

function selectAllSupported() {
  gameConfigs.value.forEach((game) => {
    game.selected = true;
    game.paths.forEach((p) => {
      p.selected = true;
    });
  });
}

function applySelectionByCheck() {
  gameConfigs.value.forEach((game) => {
    game.paths.forEach((p) => {
      // Registry paths: select if supported and exists
      if (p.isRegistry) {
        p.selected = !!p.check && !p.check.error && p.check.exists !== false;
        return;
      }
      p.selected = !!p.check && !p.check.error && p.check.exists === true;
    });
    game.selected = game.paths.some((p) => p.selected);
  });
}

async function selectByCheck() {
  const hasAnyCheck = gameConfigs.value.some((g) => g.paths.some((p) => p.check));
  if (!hasAnyCheck) {
    await checkAllPaths(true);
    return;
  }
  applySelectionByCheck();
}
</script>

<style scoped>
.batch-dialog-content {
  height: 70vh;
  min-height: 320px;
  overflow-y: auto;
}

.info-alert {
  margin-bottom: 16px;
}

.store-user-id-row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 12px;
}

.store-user-id-label {
  font-size: 13px;
  font-weight: 500;
  white-space: nowrap;
}

.store-user-id-select {
  width: 280px;
}

.store-user-id-hint {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}

.toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px;
  margin-bottom: 8px;
}

.search-input {
  flex: 1;
  min-width: 240px;
}

.only-selected-checkbox {
  white-space: nowrap;
}

.summary-tag {
  white-space: nowrap;
}

.toolbar-actions {
  margin-left: auto;
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
}

.games-list {
  margin-top: 16px;
}

.game-header {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
}

.game-checkbox {
  flex-shrink: 0;
}

.game-name {
  flex: 1;
  font-weight: 500;
}

.path-count {
  flex-shrink: 0;
}

.path-item {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}

.path-checkbox {
  flex-shrink: 0;
}

.path-input {
  flex: 1;
}

.path-tag {
  flex-shrink: 0;
  margin-left: 4px;
}

.registry-tag {
  margin-left: 0;
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
