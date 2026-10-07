<script setup lang="ts">
import { computed, ref } from 'vue';
import { FolderOpen } from '@lucide/vue';
import { commands, type Device, type Game } from '../api/commands';
import { notifyError } from '../composables/useActivityCenter';
import { $t } from '../i18n';
import { KButton, KSelect } from '../ui/kit';

const props = defineProps<{ device: Device; games: Game[] }>();
const emit = defineEmits<{ updated: [] }>();
const chosen = ref<Record<number, string | number>>({});
const busy = ref(false);
const records = computed(
  () =>
    props.device.resources?.flatMap((resource) =>
      resource.kind.type === 'gameInstallation'
        ? [{ id: resource.id, path: resource.kind.path }]
        : []
    ) ?? []
);
const games = computed(() =>
  props.games.map((game) => ({ value: game.storage_key || game.name, label: game.name }))
);

function selected(id: number) {
  if (chosen.value[id] != null) return chosen.value[id];
  const matches = props.games.filter((game) =>
    game.device_bindings?.[props.device.id]?.installationIds?.includes(`resource:${id}`)
  );
  return matches.length === 1 ? matches[0]?.storage_key || matches[0]?.name : undefined;
}

async function assign(id: number, path: string) {
  const game = props.games.find((game) => (game.storage_key || game.name) === selected(id));
  if (!game) return;
  busy.value = true;
  try {
    const result = await commands.setGameDeviceBinding(game.storage_key || game.name, {
      ...game.device_bindings?.[props.device.id],
      installationPath: path,
    });
    if (result.status === 'error') notifyError(result.error);
    else emit('updated');
  } catch (error) {
    notifyError(String(error));
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <section v-if="records.length" data-testid="legacy-installations" class="min-w-0">
    <div class="mb-3 flex items-center gap-2 border-b border-border pb-2">
      <FolderOpen :size="15" class="text-text-dim" aria-hidden="true" />
      <h2 class="text-sm font-semibold text-text">{{ $t('local_upgrade.installations_title') }}</h2>
    </div>
    <p class="mb-3 text-xs leading-relaxed text-text-dim">
      {{ $t('local_upgrade.installations_hint', { variable: '<base />' }) }}
    </p>
    <div
      v-for="record in records"
      :key="record.id"
      data-testid="legacy-installation"
      class="border-b border-border py-3 first:pt-0 last:border-0 last:pb-0"
    >
      <p class="mb-2 break-all text-sm text-text">{{ record.path }}</p>
      <div class="flex flex-wrap items-center gap-2">
        <KSelect
          :model-value="selected(record.id)"
          :options="games"
          :disabled="busy"
          :placeholder="$t('local_upgrade.choose_game')"
          :aria-label="$t('local_upgrade.choose_game')"
          class="min-w-0 flex-1 basis-48 [&>span:first-child]:truncate"
          @update:model-value="
            (value) => {
              if (value != null) chosen[record.id] = value;
            }
          "
        />
        <KButton
          :disabled="busy || selected(record.id) == null"
          @click="assign(record.id, record.path)"
        >
          {{ $t('local_upgrade.assign_installation') }}
        </KButton>
      </div>
    </div>
  </section>
</template>
