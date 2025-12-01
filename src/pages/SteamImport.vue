<script lang="ts" setup>
import { ref, computed } from 'vue';
import { commands, type SteamGame } from '../bindings';
import { $t } from '../i18n';
import { Download, Check, Loading } from '@element-plus/icons-vue';

const { showError, showSuccess, showWarning } = useNotification();
const router = useRouter();

const steamGames = ref<SteamGame[]>([]);
const selectedGames = ref<Set<string>>(new Set());
const isDetecting = ref(false);
const isImporting = ref(false);

// Check if all games are selected
const allSelected = computed(() => {
  return steamGames.value.length > 0 && selectedGames.value.size === steamGames.value.length;
});

// Check if some (but not all) games are selected
const indeterminate = computed(() => {
  return selectedGames.value.size > 0 && selectedGames.value.size < steamGames.value.length;
});

// Detect Steam games
async function detectSteamGames() {
  isDetecting.value = true;
  try {
    const result = await commands.detectSteamGames();
    if (result.status === 'ok') {
      steamGames.value = result.data;
      selectedGames.value.clear();
      
      if (steamGames.value.length === 0) {
        showWarning({ message: $t('steam_import.no_games_found') });
      } else {
        showSuccess({ 
          message: $t('steam_import.detect_success', { count: steamGames.value.length }) 
        });
      }
    } else {
      showError({ message: result.error || $t('steam_import.detect_failed') });
    }
  } catch (error) {
    console.error('Failed to detect Steam games:', error);
    showError({ message: $t('steam_import.detect_failed') });
  } finally {
    isDetecting.value = false;
  }
}

// Toggle game selection
function toggleGameSelection(appId: string) {
  if (selectedGames.value.has(appId)) {
    selectedGames.value.delete(appId);
  } else {
    selectedGames.value.add(appId);
  }
}

// Select all games
function selectAll() {
  steamGames.value.forEach((game) => selectedGames.value.add(game.app_id));
}

// Deselect all games
function deselectAll() {
  selectedGames.value.clear();
}

// Toggle all games selection
function toggleAll() {
  if (allSelected.value) {
    deselectAll();
  } else {
    selectAll();
  }
}

// Import selected games
async function importSelectedGames() {
  if (selectedGames.value.size === 0) {
    showWarning({ message: $t('steam_import.no_games_found') });
    return;
  }

  isImporting.value = true;
  try {
    const gamesToImport = steamGames.value.filter((game) =>
      selectedGames.value.has(game.app_id)
    );

    const result = await commands.importSteamGames(gamesToImport);
    if (result.status === 'ok') {
      showSuccess({
        message: $t('steam_import.import_success', { count: gamesToImport.length }),
      });
      // Navigate back to home or management page
      router.push('/');
    } else {
      showError({ message: result.error || $t('steam_import.import_failed') });
    }
  } catch (error) {
    console.error('Failed to import Steam games:', error);
    showError({ message: $t('steam_import.import_failed') });
  } finally {
    isImporting.value = false;
  }
}

// Go back
function goBack() {
  router.back();
}
</script>

<template>
  <div class="steam-import-page">
    <el-card>
      <template #header>
        <div class="card-header">
          <span class="title">{{ $t('steam_import.title') }}</span>
        </div>
      </template>

      <div class="actions">
        <el-button
          type="primary"
          :icon="Download"
          :loading="isDetecting"
          @click="detectSteamGames"
        >
          {{ isDetecting ? $t('steam_import.detecting') : $t('steam_import.detect_button') }}
        </el-button>

        <el-button
          v-if="steamGames.length > 0"
          type="success"
          :icon="Check"
          :loading="isImporting"
          :disabled="selectedGames.size === 0"
          @click="importSelectedGames"
        >
          {{ isImporting ? $t('steam_import.importing') : $t('steam_import.import_button') }}
        </el-button>

        <el-button @click="goBack">
          {{ $t('common.cancel') }}
        </el-button>
      </div>

      <div v-if="steamGames.length > 0" class="games-list">
        <div class="list-header">
          <el-checkbox
            :model-value="allSelected"
            :indeterminate="indeterminate"
            @change="toggleAll"
          >
            {{ $t('steam_import.game_name') }} ({{ selectedGames.size }} / {{ steamGames.length }})
          </el-checkbox>
          <div class="header-actions">
            <el-link type="primary" @click="selectAll">{{ $t('steam_import.select_all') }}</el-link>
            <el-divider direction="vertical" />
            <el-link type="primary" @click="deselectAll">{{ $t('steam_import.deselect_all') }}</el-link>
          </div>
        </div>

        <el-table :data="steamGames" style="width: 100%" max-height="500">
          <el-table-column width="55">
            <template #default="scope">
              <el-checkbox
                :model-value="selectedGames.has(scope.row.app_id)"
                @change="toggleGameSelection(scope.row.app_id)"
              />
            </template>
          </el-table-column>

          <el-table-column :label="$t('steam_import.game_name')" prop="name" min-width="200" />

          <el-table-column :label="$t('steam_import.save_paths')" min-width="300">
            <template #default="scope">
              <div v-if="scope.row.save_paths.length > 0" class="save-paths">
                <el-tag
                  v-for="(savePath, index) in scope.row.save_paths"
                  :key="index"
                  size="small"
                  style="margin: 2px"
                >
                  {{ savePath.path }}
                </el-tag>
              </div>
              <span v-else class="no-save-paths">{{ $t('steam_import.no_save_paths') }}</span>
            </template>
          </el-table-column>
        </el-table>
      </div>

      <el-empty
        v-else-if="!isDetecting"
        :description="$t('steam_import.no_games_found')"
      />
    </el-card>
  </div>
</template>

<style scoped>
.steam-import-page {
  padding: 20px;
  max-width: 1200px;
  margin: 0 auto;
}

.card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.title {
  font-size: 20px;
  font-weight: bold;
}

.actions {
  display: flex;
  gap: 10px;
  margin-bottom: 20px;
}

.games-list {
  margin-top: 20px;
}

.list-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 10px;
  padding: 10px;
  background-color: var(--el-fill-color-light);
  border-radius: 4px;
}

.header-actions {
  display: flex;
  align-items: center;
  gap: 5px;
}

.save-paths {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.no-save-paths {
  color: var(--el-text-color-secondary);
  font-style: italic;
}
</style>
