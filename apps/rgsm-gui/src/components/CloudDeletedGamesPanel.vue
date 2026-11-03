<script setup lang="ts">
import { RefreshCw } from '@lucide/vue';

import { commands, type DeletedCloudGameView } from '../api/commands';
import { useCloudLibrary } from '../composables/useCloudLibrary';
import { $t } from '../i18n';
import { KButton, KTag } from '../ui/kit';
import { notifyError, notifySuccess } from '../composables/useActivityCenter';

const emit = defineEmits<{
  updated: [];
}>();

const { deletedGames: games } = useCloudLibrary();
const retrying = ref('');

async function retry(game: DeletedCloudGameView) {
  retrying.value = game.game_id;
  try {
    const result = await commands.permanentlyDeleteCloudGame(game.game_id, true);
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.games.delete_incomplete'), result.error);
    } else {
      notifySuccess(
        $t('sync_settings.archives.games.delete_success', {
          snapshots: result.data.removed_snapshots,
        })
      );
    }
    emit('updated');
  } finally {
    retrying.value = '';
  }
}
</script>

<template>
  <section v-if="games.length > 0" class="flex flex-col gap-2">
    <div>
      <strong class="text-sm font-medium text-text">{{
        $t('sync_settings.archives.games.deleted_title')
      }}</strong>
    </div>
    <div
      v-for="game in games"
      :key="game.game_id"
      class="flex items-center justify-between gap-3 border-t border-border pt-2"
    >
      <div class="flex min-w-0 flex-col">
        <strong class="truncate text-sm text-text">{{ game.name }}</strong>
        <small class="truncate font-mono text-[11px] text-text-dim">{{ game.game_id }}</small>
      </div>
      <div class="flex shrink-0 items-center gap-1.5">
        <KTag :tone="game.deletion_incomplete ? 'warning' : 'neutral'">
          {{
            game.deletion_incomplete
              ? $t('sync_settings.archives.games.incomplete')
              : $t('sync_settings.archives.games.removed')
          }}
        </KTag>
        <KButton
          v-if="game.deletion_incomplete"
          variant="ghost"
          size="sm"
          class="text-warning"
          :loading="retrying === game.game_id"
          @click="retry(game)"
        >
          <template #icon><RefreshCw :size="13" aria-hidden="true" /></template>
          {{ $t('sync_settings.archives.games.retry') }}
        </KButton>
      </div>
    </div>
  </section>
</template>
