<script setup lang="ts">
import { computed, ref, type Component } from 'vue';
import { useRouter } from 'vue-router';
import {
  commands,
  type CloudArchiveGameView,
  type CloudArchiveLibraryView,
  type SyncMode,
} from '../api/commands';
import { notifyError, notifySuccess } from '../composables/useActivityCenter';
import { getGameManagementPath } from '../composables/useGameManagementRoute';
import { $t } from '../i18n';
import { Archive, ChevronDown, Hand, Trash2, Zap } from '@lucide/vue';
import { KInput, KMenu, KSwitch, KTag, type KMenuEntry } from '../ui/kit';

const router = useRouter();
const feedback = useFeedback();
const library = ref<CloudArchiveLibraryView | null>(null);
const search = ref('');
const modeGame = ref<CloudArchiveGameView | null>(null);
const pendingMode = ref<SyncMode>('cloud_backup');
const progressGame = ref<CloudArchiveGameView | null>(null);
const busyGameId = ref('');
const fleet = ref<{ load: () => Promise<void> } | null>(null);

const games = computed(() => {
  const keyword = search.value.trim().toLowerCase();
  const rows = library.value?.games ?? [];
  if (!keyword) return rows;
  return rows.filter((game) => game.name.toLowerCase().includes(keyword));
});

const MODE_META: Record<string, { labelKey: string; descKey: string; icon: Component }> = {
  manual: {
    labelKey: 'sync_settings.overview.mode_manual',
    descKey: 'sync_settings.archives.manual_sync_description',
    icon: Hand,
  },
  cloud_backup: {
    labelKey: 'sync_settings.overview.mode_snapshot',
    descKey: 'sync_settings.archives.snapshot_sync_description',
    icon: Archive,
  },
  multi_device_sync: {
    labelKey: 'sync_settings.overview.mode_live',
    descKey: 'sync_settings.archives.live_save_sync_description',
    icon: Zap,
  },
};

function modeIcon(mode: SyncMode) {
  return MODE_META[mode]?.icon ?? Hand;
}
function modeLabel(mode: SyncMode) {
  return $t(MODE_META[mode]?.labelKey ?? 'sync_settings.overview.mode_manual');
}
function modeEntries(game: CloudArchiveGameView): KMenuEntry[] {
  return (['manual', 'cloud_backup', 'multi_device_sync'] as const).map((mode) => ({
    type: 'item',
    key: mode,
    label: $t(MODE_META[mode].labelKey),
    description: $t(MODE_META[mode].descKey),
    icon: MODE_META[mode].icon,
    active: game.sync_mode === mode,
  }));
}

function normalizeMode(mode: SyncMode | string): 'manual' | 'cloud_backup' | 'multi_device_sync' {
  if (mode === 'cloud_backup') return 'cloud_backup';
  if (mode === 'multi_device_sync') return 'multi_device_sync';
  return 'manual';
}

function needsProgressChoice(game: CloudArchiveGameView) {
  return game.managed && game.cloud_sync_enabled && game.requires_choice;
}

function syncStatus(game: CloudArchiveGameView) {
  if (!game.managed || !game.cloud_sync_enabled) return 'disabled';
  if (game.requires_choice) return 'conflict';
  if (game.has_update) return 'update_available';
  return 'synced';
}

function statusTone(status: ReturnType<typeof syncStatus>) {
  if (status === 'synced') return 'success' as const;
  if (status === 'update_available') return 'accent' as const;
  if (status === 'conflict') return 'warning' as const;
  return 'neutral' as const;
}

function statusLabel(status: ReturnType<typeof syncStatus>) {
  return $t(`sync_settings.overview.status_${status}`);
}

async function reload() {
  await fleet.value?.load();
}

function onLibraryLoaded(next: CloudArchiveLibraryView) {
  library.value = next;
}

async function changeMode(game: CloudArchiveGameView, mode: string | number | boolean) {
  if (!game.managed || !game.cloud_sync_enabled) return;
  const next = normalizeMode(String(mode));
  if (next === normalizeMode(game.sync_mode)) return;
  if (next === 'cloud_backup' || next === 'multi_device_sync') {
    modeGame.value = game;
    pendingMode.value = next as SyncMode;
    return;
  }
  const result = await commands.setGameSyncMode(
    game.game_id,
    next as SyncMode,
    'keep_remote',
    null,
    true
  );
  if (result.status === 'error') {
    notifyError($t('sync_settings.archives.mode_change_failed'), result.error);
    await reload();
    return;
  }
  notifySuccess($t('sync_settings.archives.mode_changed'));
  await reload();
}

async function setCloudEnabled(game: CloudArchiveGameView, enabled: boolean) {
  if (!game.managed) return;
  busyGameId.value = game.game_id;
  try {
    const result = await commands.setGameSyncMode(
      game.game_id,
      normalizeMode(game.sync_mode) as SyncMode,
      'keep_remote',
      null,
      enabled
    );
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.mode_change_failed'), result.error);
      return;
    }
    notifySuccess($t('sync_settings.archives.mode_changed'));
    await reload();
  } finally {
    busyGameId.value = '';
  }
}

async function permanentlyDelete(game: CloudArchiveGameView) {
  try {
    await feedback.confirm(
      $t('sync_settings.archives.games.delete_confirm', { game: game.name }),
      $t('sync_settings.archives.games.delete_title'),
      {
        confirmButtonText: $t('sync_settings.archives.games.delete_action'),
        cancelButtonText: $t('sync_settings.cancel'),
        type: 'error',
      }
    );
  } catch {
    return;
  }
  const result = await commands.permanentlyDeleteCloudGame(game.game_id, true);
  if (result.status === 'error') {
    notifyError($t('sync_settings.archives.games.delete_incomplete'), result.error);
    return;
  }
  notifySuccess(
    $t('sync_settings.archives.games.delete_success', {
      snapshots: result.data.removed_snapshots,
    })
  );
  await reload();
}

function distribution(game: CloudArchiveGameView) {
  const localOnly = game.local_only_count ?? 0;
  const cloudOnly = game.cloud_only_count ?? 0;
  const both = game.both_available_count ?? 0;
  const otherDevice = game.other_device_only_count ?? 0;
  const unavailable = game.unavailable_count ?? 0;
  const total = localOnly + cloudOnly + both + otherDevice + unavailable;
  if (total === 0) return $t('sync_settings.overview.dist_empty');
  const parts: string[] = [];
  const local = localOnly + both;
  const cloud = cloudOnly + both;
  if (local > 0) parts.push($t('sync_settings.overview.dist_local', { count: local }));
  if (cloud > 0) parts.push($t('sync_settings.overview.dist_cloud', { count: cloud }));
  if (otherDevice > 0)
    parts.push($t('sync_settings.overview.dist_other_device', { count: otherDevice }));
  if (unavailable > 0) parts.push($t('sync_settings.overview.dist_lost', { count: unavailable }));
  return parts.join(' \u00b7 ');
}

function openGame(game: CloudArchiveGameView) {
  void router.push(getGameManagementPath(game.name));
}
</script>

<template>
  <section>
    <CloudArchivePanel ref="fleet" @loaded="onLibraryLoaded" />

    <KInput
      v-model="search"
      class="mb-3 w-72"
      :placeholder="$t('sync_settings.overview.search')"
      :aria-label="$t('sync_settings.overview.search')"
    />

    <div class="rounded-md border border-border">
      <div
        class="grid grid-cols-[minmax(0,1fr)_5.5rem_8.5rem_16rem_4.5rem_2.25rem] items-center gap-3 border-b border-border px-3 py-2 text-xs font-medium text-text-dim"
      >
        <span>{{ $t('sync_settings.overview.game_name') }}</span>
        <span class="text-center">{{ $t('sync_settings.overview.status') }}</span>
        <span>{{ $t('sync_settings.overview.mode') }}</span>
        <span>{{ $t('sync_settings.overview.distribution') }}</span>
        <span class="text-center">{{ $t('sync_settings.overview.local_sync') }}</span>
        <span class="sr-only">{{ $t('sync_settings.overview.delete_game') }}</span>
      </div>

      <div v-if="games.length === 0" class="px-3 py-8 text-center text-sm text-text-dim">
        {{
          library && library.games.length === 0
            ? $t('sync_settings.overview.no_games')
            : $t('sync_settings.overview.no_matches')
        }}
      </div>

      <div
        v-for="game in games"
        :key="game.game_id"
        class="grid grid-cols-[minmax(0,1fr)_5.5rem_8.5rem_16rem_4.5rem_2.25rem] items-center gap-3 border-b border-border px-3 py-2.5 last:border-b-0"
      >
        <div class="min-w-0">
          <button
            type="button"
            class="block max-w-full cursor-pointer truncate border-none bg-transparent p-0 text-left text-sm font-medium text-text transition-colors hover:text-accent"
            @click="openGame(game)"
          >
            {{ game.name }}
          </button>
          <button
            v-if="needsProgressChoice(game)"
            type="button"
            class="mt-0.5 block cursor-pointer border-none bg-transparent p-0 text-left text-xs text-warning transition-colors hover:brightness-110"
            @click="progressGame = game"
          >
            {{ $t('sync_settings.overview.progress_needed') }}
          </button>
        </div>

        <div class="flex justify-center">
          <KTag :tone="statusTone(syncStatus(game))">{{ statusLabel(syncStatus(game)) }}</KTag>
        </div>

        <KMenu
          :entries="modeEntries(game)"
          :aria-label="$t('sync_settings.overview.mode')"
          @select="(key: string) => changeMode(game, key)"
        >
          <button
            type="button"
            class="inline-flex h-7 cursor-pointer items-center gap-1.5 rounded-sm border border-transparent bg-transparent px-2 text-xs text-text transition-colors hover:bg-surface-2 focus-visible:outline-2 focus-visible:outline-accent disabled:cursor-not-allowed disabled:opacity-50"
            :disabled="!game.managed || !game.cloud_sync_enabled"
            :aria-label="$t('sync_settings.overview.mode')"
          >
            <component :is="modeIcon(game.sync_mode)" :size="13" aria-hidden="true" />
            <span>{{ modeLabel(game.sync_mode) }}</span>
            <ChevronDown :size="11" class="text-text-dim" aria-hidden="true" />
          </button>
        </KMenu>

        <div class="truncate text-xs text-text-dim">
          {{ distribution(game) }}
        </div>

        <div class="flex justify-center">
          <KSwitch
            :model-value="game.managed && game.cloud_sync_enabled"
            :disabled="!game.managed || busyGameId === game.game_id"
            :aria-label="$t('sync_settings.overview.local_sync')"
            @update:model-value="setCloudEnabled(game, Boolean($event))"
          />
        </div>

        <button
          type="button"
          class="inline-flex h-7 w-7 cursor-pointer items-center justify-center rounded-sm border border-border bg-surface text-text-dim transition-colors hover:border-danger hover:bg-danger/10 hover:text-danger focus-visible:outline-2 focus-visible:outline-accent"
          :aria-label="$t('sync_settings.overview.delete_game')"
          @click="permanentlyDelete(game)"
        >
          <Trash2 :size="14" aria-hidden="true" />
        </button>
      </div>
    </div>

    <V2ConflictReviewDialog
      v-if="progressGame"
      :model-value="progressGame !== null"
      :game-id="progressGame.game_id"
      :game-name="progressGame.name"
      @update:model-value="progressGame = null"
      @resolved="
        progressGame = null;
        reload();
      "
    />
    <CloudSyncModeDialog v-model:game="modeGame" :mode="pendingMode" @updated="reload" />
  </section>
</template>
