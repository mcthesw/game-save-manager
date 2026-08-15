<template>
  <KDialog
    v-model:open="dialogVisible"
    :title="$t('game_import.title')"
    :width="880"
    :dismissable="false"
  >
    <div class="relative flex h-[70vh] min-h-80 flex-col gap-3 overflow-hidden">
      <!-- Search and filter bar -->
      <div class="flex flex-wrap items-center gap-x-4 gap-y-2">
        <div class="relative max-w-96 min-w-48 flex-1">
          <Search
            :size="14"
            class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-text-dim"
            aria-hidden="true"
          />
          <KInput
            v-model="searchText"
            class="w-full pl-8"
            :placeholder="$t('game_import.search_placeholder')"
            :aria-label="$t('game_import.search_placeholder')"
          />
        </div>
        <KCheckbox v-model="hideManaged" class="shrink-0 whitespace-nowrap">{{
          $t('game_import.hide_managed')
        }}</KCheckbox>
        <KCheckbox v-model="localOnly" class="shrink-0 whitespace-nowrap">{{
          $t('game_import.local_only')
        }}</KCheckbox>
        <div class="ml-auto text-sm text-text-dim">
          {{ $t('game_import.total_games', { count: filteredGames.length }) }}
        </div>
      </div>

      <KAlert tone="info">{{ $t('game_import.description') }}</KAlert>

      <!-- Game list -->
      <div class="min-h-0 flex-1 overflow-x-hidden overflow-y-auto rounded-sm border border-border">
        <div
          class="sticky top-0 flex items-center gap-3 border-b border-border bg-surface-2 px-3 py-2 text-xs text-text-dim"
        >
          <KCheckbox
            v-model="selectAllState"
            :aria-label="$t('game_import.select_all')"
            :disabled="selectableNames.size === 0"
          />
          <span class="min-w-0 flex-1">{{ $t('game_import.game_name') }}</span>
          <span class="w-28 shrink-0">{{ $t('game_import.steam_id') }}</span>
          <span class="w-24 shrink-0 text-right">{{ $t('game_import.save_paths_count') }}</span>
        </div>
        <div
          v-for="game in paginatedGames"
          :key="game.name"
          class="flex items-center gap-3 border-b border-border px-3 py-1.5 last:border-b-0"
          :class="game.isManaged ? 'opacity-60' : 'cursor-pointer hover:bg-surface-2'"
          @click="toggleRow(game)"
        >
          <KCheckbox
            :model-value="isSelected(game)"
            :disabled="!checkSelectable(game)"
            :aria-label="game.name"
            @click.stop
            @update:model-value="toggleRow(game)"
          />
          <div class="flex min-w-0 flex-1 items-center gap-2">
            <span class="truncate text-sm text-text">{{ game.name }}</span>
            <KTag v-if="game.isManaged" class="shrink-0">{{ $t('game_import.managed') }}</KTag>
          </div>
          <span class="w-28 shrink-0 font-mono text-xs text-text-dim">
            {{ game.steamId || '-' }}
          </span>
          <span class="w-24 shrink-0 text-right text-xs text-text-dim">
            {{ game.savePathsCount }}
          </span>
        </div>
        <div
          v-if="paginatedGames.length === 0 && !loading"
          class="px-3 py-8 text-center text-sm text-text-dim"
        >
          {{ $t('common.no_data') }}
        </div>
      </div>

      <!-- Pagination -->
      <div class="flex items-center justify-end gap-2 text-xs text-text-dim">
        <KSelect
          v-model="pageSizeModel"
          size="sm"
          :options="pageSizeOptions"
          :aria-label="$t('game_import.per_page', { count: pageSize })"
          class="w-28"
        />
        <span class="whitespace-nowrap font-mono">{{ pageRangeText }}</span>
        <KButton
          variant="ghost"
          size="sm"
          :disabled="currentPage <= 1"
          :aria-label="$t('common.prev_page')"
          @click="currentPage--"
        >
          <ChevronLeft :size="14" aria-hidden="true" />
        </KButton>
        <KButton
          variant="ghost"
          size="sm"
          :disabled="currentPage >= pageCount"
          :aria-label="$t('common.next_page')"
          @click="currentPage++"
        >
          <ChevronRight :size="14" aria-hidden="true" />
        </KButton>
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
      <KButton @click="handleClose">{{ $t('common.cancel') }}</KButton>
      <KButton variant="primary" :disabled="selectedGames.length === 0" @click="handleImport">
        {{ $t('game_import.import_selected', { count: selectedGames.length }) }}
      </KButton>
    </template>
  </KDialog>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { ChevronLeft, ChevronRight, LoaderCircle, Search } from '@lucide/vue';
import { $t } from '../i18n';
import { KAlert, KButton, KCheckbox, KDialog, KInput, KSelect, KTag } from '../ui/kit';

// Import the type from bindings
import type { ImportableGame } from '../api/commands';

const props = defineProps({
  modelValue: {
    type: Boolean,
    required: true,
  },
  games: {
    type: Array as () => ImportableGame[],
    default: () => [],
  },
  loading: {
    type: Boolean,
    default: false,
  },
});

const emit = defineEmits<{
  (event: 'update:modelValue' | 'toggleLocal', value: boolean): void;
  (event: 'import', games: ImportableGame[]): void;
}>();

// Dialog visibility
const dialogVisible = computed({
  get: () => props.modelValue,
  set: (value: boolean) => emit('update:modelValue', value),
});

// Search and filter
const searchText = ref('');
const hideManaged = ref(true);
const localOnly = ref(true); // Default to showing local games only
const selectedGames = ref<ImportableGame[]>([]);

// Watch localOnly changes and emit event
watch(localOnly, (newValue) => {
  emit('toggleLocal', newValue);
});

// Pagination
const currentPage = ref(1);
const pageSize = ref(100);

const pageSizeOptions = [50, 100, 200, 500].map((size) => ({
  value: String(size),
  label: $t('game_import.per_page', { count: size }),
}));
const pageSizeModel = computed({
  get: () => String(pageSize.value),
  set: (value: string) => {
    pageSize.value = Number(value) || 100;
    currentPage.value = 1;
  },
});

// Filtered games based on search and hide managed
const filteredGames = computed(() => {
  let result = props.games;

  // Filter by search text
  if (searchText.value) {
    const searchLower = searchText.value.toLowerCase();
    result = result.filter((game) => game.name.toLowerCase().includes(searchLower));
  }

  // Filter out managed games if checkbox is checked
  if (hideManaged.value) {
    result = result.filter((game) => !game.isManaged);
  }

  return result;
});

// Paginated games
const pageCount = computed(() =>
  Math.max(1, Math.ceil(filteredGames.value.length / pageSize.value))
);
const paginatedGames = computed(() => {
  const start = (currentPage.value - 1) * pageSize.value;
  const end = start + pageSize.value;
  return filteredGames.value.slice(start, end);
});
const pageRangeText = computed(() => {
  const total = filteredGames.value.length;
  if (total === 0) return '0-0 / 0';
  const start = (currentPage.value - 1) * pageSize.value + 1;
  const end = Math.min(total, currentPage.value * pageSize.value);
  return `${start}-${end} / ${total}`;
});

// Reset to first page when filters change
watch([searchText, hideManaged], () => {
  currentPage.value = 1;
});

// Check if a row can be selected (not managed)
function checkSelectable(row: ImportableGame) {
  return !row.isManaged;
}

// Selection handling (replaces el-table selection column)
const selectedNames = computed(() => new Set(selectedGames.value.map((game) => game.name)));

const selectableNames = computed(
  () =>
    new Set(filteredGames.value.filter((game) => checkSelectable(game)).map((game) => game.name))
);

function isSelected(game: ImportableGame) {
  return selectedNames.value.has(game.name);
}

function toggleRow(game: ImportableGame) {
  if (!checkSelectable(game)) return;
  if (isSelected(game)) {
    selectedGames.value = selectedGames.value.filter((item) => item.name !== game.name);
  } else {
    selectedGames.value = [...selectedGames.value, game];
  }
}

const selectAllState = computed<boolean | 'indeterminate'>({
  get: () => {
    const selectable = selectableNames.value;
    if (selectable.size === 0) return false;
    const selected = [...selectable].filter((name) => selectedNames.value.has(name)).length;
    if (selected === 0) return false;
    return selected === selectable.size ? true : 'indeterminate';
  },
  set: (value) => {
    if (value === true) {
      const toAdd = filteredGames.value.filter(
        (game) => checkSelectable(game) && !selectedNames.value.has(game.name)
      );
      selectedGames.value = [...selectedGames.value, ...toAdd];
    } else {
      selectedGames.value = selectedGames.value.filter(
        (game) => !selectableNames.value.has(game.name)
      );
    }
  },
});

// Handle close
function handleClose() {
  emit('update:modelValue', false);
}

// Handle import
function handleImport() {
  emit('import', selectedGames.value);
}
</script>
