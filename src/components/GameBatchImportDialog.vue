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
                Registry
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
import { commands, type ImportableGame, type SavePath, type PathCheckResult } from '../bindings';

/** Check if a path is a Windows registry path (not supported for backup) */
function isRegistryPath(path: string): boolean {
  return path.startsWith('REGISTRY:') || path.startsWith('HKEY_');
}

interface GameConfig {
  name: string;
  customName: string;
  selected: boolean;
  paths: Array<{
    path: string;
    tags: string[];
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
  (event: 'confirm', configs: GameConfig[]): void;
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

// Initialize game configs when games or paths change
watch(
  () => [props.games, props.gamePaths],
  () => {
    gameConfigs.value = props.games.map((game) => {
      const paths = props.gamePaths[game.name] || [];
      return {
        name: game.name,
        customName: game.name,
        selected: true,
        paths: paths.map((p) => ({
          path: p.path,
          tags: p.tags,
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
  (open) => {
    if (!open) return;
    autoCheckedOnce.value = false;
  }
);

// Auto-check paths and use results to set default selection after paths finish loading
watch(
  () => [dialogVisible.value, props.loading],
  async ([open, loading]) => {
    if (!open || loading) return;
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
  emit('confirm', selected);
  emit('update:modelValue', false);
}

async function checkAllPaths(applySelection: boolean = false) {
  const indexMap: Array<{ gameIndex: number; pathIndex: number }> = [];
  const paths: string[] = [];

  gameConfigs.value.forEach((game, gameIndex) => {
    game.paths.forEach((_path, pathIndex) => {
      indexMap.push({ gameIndex, pathIndex });
      paths.push(game.paths[pathIndex]?.path ?? '');
    });
  });

  if (!paths.length) return;

  isChecking.value = true;
  try {
    const result = await commands.checkPaths(paths);
    if (result.status === 'ok') {
      const checks = result.data as PathCheckResult[];
      checks.forEach((c, i) => {
        const idx = indexMap[i];
        if (!idx) return;
        const pathItem = gameConfigs.value[idx.gameIndex]?.paths[idx.pathIndex];
        if (!pathItem) return;
        // Map enum variants to the check object format
        if (c.status === 'ok') {
          pathItem.check = { resolvedPath: c.resolvedPath, exists: true };
        } else if (c.status === 'notFound') {
          pathItem.check = { resolvedPath: c.resolvedPath, exists: false };
        } else if (c.status === 'registryPath') {
          pathItem.check = c.supported
            ? { resolvedPath: c.rawPath, exists: c.exists }
            : { error: 'Registry paths are not supported on this platform' };
        } else if (c.status === 'resolveFailed') {
          pathItem.check = { error: c.error };
        }
      });
      if (applySelection) {
        applySelectionByCheck();
      }
    } else {
      gameConfigs.value.forEach((game) => {
        game.paths.forEach((p) => {
          p.check = { error: result.error };
        });
      });
      if (applySelection) {
        selectAllSupported();
      }
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
