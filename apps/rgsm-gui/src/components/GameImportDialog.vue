<template>
  <el-dialog
    v-model="dialogVisible"
    :title="$t('game_import.title')"
    width="80%"
    top="5vh"
    :close-on-click-modal="false"
    append-to-body
    @close="handleClose"
  >
    <div v-loading="loading" class="import-dialog-content">
      <!-- Search and filter bar -->
      <div class="filter-bar">
        <el-input
          v-model="searchText"
          :placeholder="$t('game_import.search_placeholder')"
          clearable
          class="search-input"
        >
          <template #prefix>
            <el-icon><Search /></el-icon>
          </template>
        </el-input>
        <el-checkbox v-model="hideManaged" class="hide-managed-checkbox">
          {{ $t('game_import.hide_managed') }}
        </el-checkbox>
        <el-checkbox v-model="localOnly" class="local-only-checkbox">
          {{ $t('game_import.local_only') }}
        </el-checkbox>
        <div class="stats">
          {{ $t('game_import.total_games', { count: filteredGames.length }) }}
        </div>
      </div>

      <el-alert type="info" :closable="false" class="about-alert">
        {{ $t('game_import.description') }}
      </el-alert>

      <!-- Game list -->
      <el-table
        ref="tableRef"
        :data="paginatedGames"
        height="100%"
        stripe
        class="game-table"
        @selection-change="handleSelectionChange"
      >
        <el-table-column type="selection" width="55" :selectable="checkSelectable" />
        <el-table-column :label="$t('game_import.game_name')" prop="name" min-width="300">
          <template #default="{ row }">
            <div class="game-name-cell">
              <span>{{ row.name }}</span>
              <el-tag v-if="row.isManaged" type="info" size="small" class="managed-tag">
                {{ $t('game_import.managed') }}
              </el-tag>
            </div>
          </template>
        </el-table-column>
        <el-table-column :label="$t('game_import.steam_id')" prop="steamId" width="120">
          <template #default="{ row }">
            {{ row.steamId || '-' }}
          </template>
        </el-table-column>
        <el-table-column
          :label="$t('game_import.save_paths_count')"
          prop="savePathsCount"
          width="150"
        />
      </el-table>

      <!-- Pagination -->
      <el-pagination
        v-model:current-page="currentPage"
        v-model:page-size="pageSize"
        :page-sizes="[50, 100, 200, 500]"
        :total="filteredGames.length"
        layout="total, sizes, prev, pager, next"
        class="pagination"
      />
    </div>

    <template #footer>
      <div class="dialog-footer">
        <el-button @click="handleClose">{{ $t('common.cancel') }}</el-button>
        <el-button type="primary" :disabled="selectedGames.length === 0" @click="handleImport">
          {{ $t('game_import.import_selected', { count: selectedGames.length }) }}
        </el-button>
      </div>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { Search } from '@element-plus/icons-vue';
import { $t } from '../i18n';

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
const paginatedGames = computed(() => {
  const start = (currentPage.value - 1) * pageSize.value;
  const end = start + pageSize.value;
  return filteredGames.value.slice(start, end);
});

// Reset to first page when filters change
watch([searchText, hideManaged], () => {
  currentPage.value = 1;
});

// Check if a row can be selected (not managed)
function checkSelectable(row: ImportableGame) {
  return !row.isManaged;
}

// Handle selection change
function handleSelectionChange(selection: ImportableGame[]) {
  selectedGames.value = selection;
}

// Handle close
function handleClose() {
  emit('update:modelValue', false);
}

// Handle import
function handleImport() {
  emit('import', selectedGames.value);
}
</script>

<style scoped>
.import-dialog-content {
  height: 70vh;
  display: flex;
  flex-direction: column;
  min-height: 320px;
  overflow: hidden;
}

.filter-bar {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-bottom: 16px;
}

.about-alert {
  margin-bottom: 12px;
}

.search-input {
  flex: 1;
  max-width: 400px;
}

.hide-managed-checkbox {
  white-space: nowrap;
}

.stats {
  margin-left: auto;
  color: var(--el-text-color-secondary);
  font-size: 14px;
}

.game-name-cell {
  display: flex;
  align-items: center;
  gap: 8px;
}

.managed-tag {
  flex-shrink: 0;
}

.pagination {
  margin-top: 16px;
  justify-content: flex-end;
}

.game-table {
  flex: 1;
  min-height: 0;
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
