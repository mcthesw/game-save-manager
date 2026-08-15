<script setup lang="ts">
import { computed, ref } from 'vue';
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
import { KButton, KInput, KSegmented, KSwitch, KTag } from '../ui/kit';

const router = useRouter();
const feedback = useFeedback();
const library = ref<CloudArchiveLibraryView | null>(null);
const search = ref('');
const modeGame = ref<CloudArchiveGameView | null>(null);
const pendingMode = ref<SyncMode>('snapshot_sync');
const progressGame = ref<CloudArchiveGameView | null>(null);
const busyGameId = ref('');
const fleet = ref<{ load: () => Promise<void> } | null>(null);

const games = computed(() => {
  const keyword = search.value.trim().toLowerCase();
  const rows = library.value?.games ?? [];
  if (!keyword) return rows;
  return rows.filter((game) => game.name.toLowerCase().includes(keyword));
});

const modeOptions = computed(() => [
  { value: 'manual', label: $t('sync_settings.overview.mode_manual') },
  { value: 'snapshot_sync', label: $t('sync_settings.overview.mode_snapshot') },
  { value: 'live_save_sync', label: $t('sync_settings.overview.mode_live') },
]);

function needsProgressChoice(game: CloudArchiveGameView) {
  return game.managed && game.requires_choice;
}

function tableRow(row: unknown): CloudArchiveGameView {
  return row as CloudArchiveGameView;
}

function syncStatus(game: CloudArchiveGameView) {
  if (!game.managed) return 'disabled';
  if (game.requires_choice) return 'conflict';
  return 'synced';
}

function statusTone(status: ReturnType<typeof syncStatus>) {
  if (status === 'synced') return 'success' as const;
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
  if (!game.managed) return;
  const next = String(mode) as SyncMode;
  if (next === game.sync_mode) return;
  if (next === 'snapshot_sync' || next === 'live_save_sync') {
    modeGame.value = game;
    pendingMode.value = next;
    return;
  }
  const result = await commands.setGameSyncMode(game.game_id, next, 'keep_remote', null);
  if (result.status === 'error') {
    notifyError($t('sync_settings.archives.mode_change_failed'), result.error);
    await reload();
    return;
  }
  notifySuccess($t('sync_settings.archives.mode_changed'));
  await reload();
}

async function setManaged(game: CloudArchiveGameView, managed: boolean) {
  if (!managed) {
    try {
      await feedback.confirm(
        $t('sync_settings.archives.device.stop_confirm'),
        $t('sync_settings.archives.device.stop_title'),
        {
          confirmButtonText: $t('sync_settings.overview.release'),
          cancelButtonText: $t('sync_settings.cancel'),
          type: 'warning',
        }
      );
    } catch {
      return;
    }
  }
  busyGameId.value = game.game_id;
  try {
    const result = await commands.setDeviceGameManaged(game.game_id, managed, !managed);
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.device.management_failed'), result.error);
      return;
    }
    notifySuccess(
      managed
        ? $t('sync_settings.archives.device.manage_success')
        : $t('sync_settings.archives.device.stop_success')
    );
    await reload();
  } finally {
    busyGameId.value = '';
  }
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
        class="grid grid-cols-[minmax(0,1fr)_6.5rem_21rem_6rem] items-center gap-3 border-b border-border px-3 py-2 text-xs font-medium text-text-dim"
      >
        <span>{{ $t('sync_settings.overview.game_name') }}</span>
        <span class="text-center">{{ $t('sync_settings.overview.status') }}</span>
        <span>{{ $t('sync_settings.overview.mode') }}</span>
        <span class="text-center">{{ $t('sync_settings.overview.local_sync') }}</span>
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
        class="grid grid-cols-[minmax(0,1fr)_6.5rem_21rem_6rem] items-center gap-3 border-b border-border px-3 py-2.5 last:border-b-0"
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

        <KSegmented
          :model-value="game.sync_mode"
          :options="modeOptions"
          :aria-label="$t('sync_settings.overview.mode')"
          class="w-full"
          :class="{ 'pointer-events-none opacity-50': !game.managed }"
          @update:model-value="changeMode(game, $event)"
        />

        <div class="flex justify-center">
          <KSwitch
            :model-value="game.managed"
            :aria-label="$t('sync_settings.overview.local_sync')"
            @update:model-value="setManaged(game, Boolean($event))"
          />
        </div>
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
