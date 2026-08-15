<template>
  <KDialog
    v-model:open="dialogVisible"
    :title="$t('game_batch_import.title', { count: games.length })"
    :width="960"
    :dismissable="false"
  >
    <div
      class="relative flex max-h-[70vh] min-h-80 flex-col gap-3 overflow-x-hidden overflow-y-auto"
    >
      <KAlert tone="info">{{ $t('game_batch_import.hint') }}</KAlert>

      <!-- Steam User ID selector -->
      <div class="flex flex-wrap items-center gap-2">
        <span class="text-xs font-medium text-text">{{
          $t('game_batch_import.store_user_id')
        }}</span>
        <div class="w-72">
          <KInput
            v-model="storeUserIdInput"
            size="sm"
            :placeholder="$t('game_batch_import.store_user_id_placeholder')"
            aria-label="Steam user ID"
            list="batch-user-id-candidates"
            mono
            @blur="handleStoreUserIdChange"
          />
          <datalist id="batch-user-id-candidates">
            <option
              v-for="c in userIdCandidates"
              :key="c.userId"
              :value="c.userId"
              :label="formatUserIdLabel(c)"
            />
          </datalist>
        </div>
        <span class="text-xs text-text-dim">{{ $t('game_batch_import.store_user_id_hint') }}</span>
      </div>

      <!-- Toolbar -->
      <div class="flex flex-wrap items-center gap-2">
        <div class="w-56">
          <KInput
            v-model="searchText"
            size="sm"
            :placeholder="$t('game_batch_import.search_placeholder')"
            :aria-label="$t('game_batch_import.search_placeholder')"
          />
        </div>
        <KCheckbox v-model="onlySelected">{{ $t('game_batch_import.only_selected') }}</KCheckbox>
        <KTag>
          {{
            $t('game_batch_import.selected_summary', {
              games: selectedCount,
              paths: selectedPathsCount,
            })
          }}
        </KTag>
        <div class="ml-auto flex flex-wrap items-center gap-1.5">
          <KButton size="sm" :disabled="isChecking" @click="checkAllPaths()">
            <LoaderCircle v-if="isChecking" :size="12" class="animate-spin" aria-hidden="true" />
            {{ $t('game_batch_import.verify_paths') }}
          </KButton>
          <KButton size="sm" @click="selectAllSupported">
            {{ $t('game_batch_import.select_all_supported') }}
          </KButton>
          <KButton size="sm" variant="primary" @click="selectByCheck">
            {{ $t('game_batch_import.select_by_check') }}
          </KButton>
          <KButton size="sm" variant="ghost" @click="expandAll">
            {{ $t('game_batch_import.expand_all') }}
          </KButton>
          <KButton size="sm" variant="ghost" @click="collapseAll">
            {{ $t('game_batch_import.collapse_all') }}
          </KButton>
        </div>
      </div>

      <!-- Games list with expand/collapse -->
      <div class="rounded-sm border border-border">
        <div
          v-for="game in filteredGameConfigs"
          :key="game.name"
          class="border-b border-border last:border-b-0"
        >
          <div
            class="flex cursor-pointer select-none items-center gap-2 px-3 py-2 hover:bg-surface-2"
            @click="toggleGameExpanded(game.name)"
          >
            <ChevronRight
              :size="14"
              class="shrink-0 text-text-dim transition-transform duration-150"
              :class="{ 'rotate-90': activeGames.includes(game.name) }"
              aria-hidden="true"
            />
            <KCheckbox v-model="game.selected" :aria-label="game.name" @click.stop />
            <span class="min-w-0 flex-1 truncate text-sm font-medium text-text">
              {{ game.name }}
            </span>
            <KTag class="shrink-0">
              {{ countSelectedPaths(game) }}/{{ game.paths.length }}
              {{ $t('game_batch_import.paths') }}
            </KTag>
          </div>

          <div
            v-if="activeGames.includes(game.name)"
            class="flex flex-col gap-3 border-t border-border px-3 py-3 pl-9"
          >
            <div>
              <label class="mb-1 block text-xs text-text-dim">{{ $t('addgame.game_name') }}</label>
              <KInput v-model="game.customName" size="sm" :placeholder="game.name" />
            </div>
            <div>
              <label class="mb-1 block text-xs text-text-dim">{{
                $t('game_batch_import.save_paths')
              }}</label>
              <div class="flex flex-col gap-1.5">
                <div
                  v-for="(path, pathIndex) in game.paths"
                  :key="pathIndex"
                  class="flex items-center gap-2"
                >
                  <KCheckbox v-model="path.selected" :aria-label="path.path" />
                  <KInput
                    v-model="path.path"
                    size="sm"
                    mono
                    :disabled="!path.selected"
                    class="min-w-0 flex-1"
                  />
                  <KTag v-if="path.isRegistry" class="shrink-0">
                    {{ $t('game_batch_import.registry') }}
                  </KTag>
                  <template v-else>
                    <KTooltip v-if="path.check?.error" :content="path.check.error">
                      <KTag tone="danger">{{ $t('game_batch_import.status_error') }}</KTag>
                    </KTooltip>
                    <KTooltip
                      v-else-if="path.check?.resolvedPath"
                      :content="path.check.resolvedPath"
                    >
                      <KTag :tone="path.check.exists ? 'success' : 'warning'">
                        {{
                          path.check.exists
                            ? $t('game_batch_import.status_exists')
                            : $t('game_batch_import.status_missing')
                        }}
                      </KTag>
                    </KTooltip>
                    <KTag v-else>{{ $t('game_batch_import.status_unchecked') }}</KTag>
                  </template>
                  <KTag v-for="tag in path.tags" :key="tag" class="shrink-0">{{ tag }}</KTag>
                </div>
              </div>
            </div>
          </div>
        </div>
        <div
          v-if="filteredGameConfigs.length === 0"
          class="px-3 py-6 text-center text-sm text-text-dim"
        >
          {{ $t('common.no_data') }}
        </div>
      </div>

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
      <KButton variant="primary" @click="handleConfirm">
        {{ $t('game_batch_import.import_selected', { count: selectedCount }) }}
      </KButton>
    </template>
  </KDialog>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { ChevronRight, LoaderCircle } from '@lucide/vue';
import { $t } from '../i18n';
import {
  commands,
  type ImportableGame,
  type SavePath,
  type PathCheckResult,
  type StoreUserIdCandidate,
} from '../api/commands';
import { error } from '../utils/logger';
import type { ManifestPathConstraints } from '../api/commands';
import { KAlert, KButton, KCheckbox, KDialog, KInput, KTag, KTooltip } from '../ui/kit';

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

function toggleGameExpanded(name: string) {
  if (activeGames.value.includes(name)) {
    activeGames.value = activeGames.value.filter((item) => item !== name);
  } else {
    activeGames.value = [...activeGames.value, name];
  }
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
