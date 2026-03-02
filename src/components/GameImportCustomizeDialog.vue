<template>
  <el-dialog
    v-model="dialogVisible"
    :title="$t('game_import_customize.title', { name: gameName })"
    width="70%"
    top="5vh"
    :close-on-click-modal="false"
    append-to-body
  >
    <div v-loading="loading" class="customize-dialog-content">
      <!-- Game name input -->
      <el-form :model="form" label-position="top">
        <el-form-item :label="$t('addgame.game_name')">
          <el-input v-model="form.gameName" :placeholder="$t('addgame.input_game_name_prompt')" />
        </el-form-item>

        <!-- Save paths table -->
        <el-form-item :label="$t('game_import_customize.save_paths')">
          <div class="verify-row">
            <el-button size="small" :loading="isChecking" @click="checkAllPaths()">
              {{ $t('game_import_customize.verify_paths') }}
            </el-button>
            <el-button size="small" @click="selectAllSupported">
              {{ $t('game_import_customize.select_all_supported') }}
            </el-button>
            <el-button size="small" type="primary" @click="selectByCheck">
              {{ $t('game_import_customize.select_by_check') }}
            </el-button>
          </div>
          <el-table
            ref="tableRef"
            :data="form.savePaths"
            stripe
            @selection-change="handleSelectionChange"
          >
            <el-table-column type="selection" width="55" :selectable="isRowSelectable" />
            <el-table-column :label="$t('game_import_customize.path')" prop="path" min-width="300">
              <template #default="{ row }">
                <div class="path-cell">
                  <el-input v-model="row.path" size="small" />
                  <el-tag
                    v-if="isRegistryPath(row.path)"
                    type="info"
                    size="small"
                    class="registry-tag"
                  >
                    Registry
                  </el-tag>
                </div>
              </template>
            </el-table-column>
            <el-table-column :label="$t('game_import_customize.tags')" width="150">
              <template #default="{ row }">
                <el-tag v-for="tag in row.tags" :key="tag" size="small" class="tag-item">
                  {{ tag }}
                </el-tag>
              </template>
            </el-table-column>
            <el-table-column :label="$t('game_import_customize.resolution')" min-width="220">
              <template #default="{ $index }">
                <div v-if="pathChecks[$index]" class="resolution-cell">
                  <el-tooltip
                    v-if="pathChecks[$index]?.error"
                    :content="pathChecks[$index]?.error"
                    placement="top"
                  >
                    <el-tag type="danger" size="small">
                      {{ $t('game_import_customize.status_error') }}
                    </el-tag>
                  </el-tooltip>
                  <template v-else>
                    <el-tooltip
                      v-if="pathChecks[$index]?.resolvedPath"
                      :content="pathChecks[$index]?.resolvedPath"
                      placement="top"
                    >
                      <el-tag
                        :type="pathChecks[$index]?.exists ? 'success' : 'warning'"
                        size="small"
                      >
                        {{
                          pathChecks[$index]?.exists
                            ? $t('game_import_customize.status_exists')
                            : $t('game_import_customize.status_missing')
                        }}
                      </el-tag>
                    </el-tooltip>
                  </template>
                </div>
                <el-tag v-else type="info" size="small">
                  {{ $t('game_import_customize.status_unchecked') }}
                </el-tag>
              </template>
            </el-table-column>
          </el-table>
        </el-form-item>

        <el-alert type="info" :closable="false" class="path-hint">
          {{ $t('game_import_customize.path_hint') }}
        </el-alert>
      </el-form>
    </div>

    <template #footer>
      <div class="dialog-footer">
        <el-button @click="handleCancel">{{ $t('common.cancel') }}</el-button>
        <el-button type="primary" @click="handleConfirm">
          {{ $t('common.confirm') }}
        </el-button>
      </div>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue';
import { $t } from '../i18n';
import { commands, type SavePath, type PathCheckResult } from '../bindings';

interface CustomizeData {
  gameName: string;
  savePaths: SavePath[];
}

const props = defineProps({
  modelValue: {
    type: Boolean,
    required: true,
  },
  gameName: {
    type: String,
    required: true,
  },
  savePaths: {
    type: Array as () => SavePath[],
    default: () => [],
  },
  loading: {
    type: Boolean,
    default: false,
  },
});

const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'confirm', data: CustomizeData): void;
}>();

const dialogVisible = computed({
  get: () => props.modelValue,
  set: (value: boolean) => emit('update:modelValue', value),
});

type ElTableLike = {
  clearSelection?: () => void;
  toggleRowSelection?: (row: SavePath, selected: boolean) => void;
};

const tableRef = ref<ElTableLike | null>(null);
const selectedPaths = ref<SavePath[]>([]);
const isChecking = ref(false);
const pathChecks = ref<Array<{ resolvedPath?: string; exists?: boolean; error?: string } | null>>(
  []
);

const form = ref<CustomizeData>({
  gameName: props.gameName,
  savePaths: [],
});

function isRegistryPath(path: string) {
  return path.startsWith('REGISTRY:') || path.startsWith('HKEY_');
}

function isRowSelectable(_row: SavePath) {
  // Registry paths are selectable on Windows (backend checks support)
  return true;
}

function handleSelectionChange(selection: SavePath[]) {
  selectedPaths.value = selection;
}

// Watch for props changes to update form
watch(
  () => [props.gameName, props.savePaths],
  () => {
    form.value = {
      gameName: props.gameName,
      savePaths: JSON.parse(JSON.stringify(props.savePaths)),
    };
    pathChecks.value = new Array(form.value.savePaths.length).fill(null);
  },
  { immediate: true, deep: true }
);

// Default-select all supported rows when dialog opens
watch(
  () => dialogVisible.value,
  async (open) => {
    if (!open) {
      selectedPaths.value = [];
      return;
    }

    await nextTick();
    await checkAllPaths(true);
  }
);

function handleCancel() {
  emit('update:modelValue', false);
}

function handleConfirm() {
  emit('confirm', {
    gameName: form.value.gameName,
    savePaths: selectedPaths.value,
  });
  emit('update:modelValue', false);
}

function selectAllSupported() {
  try {
    tableRef.value?.clearSelection?.();
    for (const row of form.value.savePaths) {
      if (isRowSelectable(row)) {
        tableRef.value?.toggleRowSelection?.(row, true);
      }
    }
  } catch {
    // ignore selection errors
  }
}

function applySelectionByCheck() {
  try {
    tableRef.value?.clearSelection?.();
    form.value.savePaths.forEach((row, index) => {
      const check = pathChecks.value[index];
      const selectable = isRowSelectable(row);
      const shouldSelect = selectable && !!check && !check.error && check.exists === true;
      if (selectable) {
        tableRef.value?.toggleRowSelection?.(row, shouldSelect);
      }
    });
  } catch {
    // ignore selection errors
  }
}

async function checkAllPaths(applySelection: boolean = false) {
  if (!form.value.savePaths.length) return;
  isChecking.value = true;
  try {
    const paths = form.value.savePaths.map((p) => p.path);
    const result = await commands.checkPaths(paths);
    if (result.status === 'ok') {
      const checks = result.data as PathCheckResult[];
      // Map enum variants to the check object format
      pathChecks.value = checks.map((c) => {
        if (c.status === 'ok') {
          return { resolvedPath: c.resolvedPath, exists: true };
        } else if (c.status === 'notFound') {
          return { resolvedPath: c.resolvedPath, exists: false };
        } else if (c.status === 'registryPath') {
          return c.supported
            ? { resolvedPath: c.rawPath, exists: c.exists }
            : { error: 'Registry paths are not supported on this platform' };
        } else if (c.status === 'resolveFailed') {
          return { error: c.error };
        }
        return {};
      });
      if (applySelection) {
        await nextTick();
        applySelectionByCheck();
      }
    } else {
      pathChecks.value = form.value.savePaths.map(() => ({ error: result.error }));
      if (applySelection) {
        await nextTick();
        selectAllSupported();
      }
    }
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    pathChecks.value = form.value.savePaths.map(() => ({ error: msg }));
    if (applySelection) {
      await nextTick();
      selectAllSupported();
    }
  } finally {
    isChecking.value = false;
  }
}

async function selectByCheck() {
  const hasAnyCheck = pathChecks.value.some((x) => x !== null);
  if (!hasAnyCheck) {
    await checkAllPaths(true);
    return;
  }
  applySelectionByCheck();
}
</script>

<style scoped>
.customize-dialog-content {
  height: 70vh;
  min-height: 320px;
  overflow-y: auto;
}

.verify-row {
  display: flex;
  justify-content: flex-end;
  margin-bottom: 8px;
}

.path-cell {
  display: flex;
  align-items: center;
  gap: 8px;
}

.registry-tag {
  flex-shrink: 0;
}

.resolution-cell {
  display: flex;
  align-items: center;
}

.tag-item {
  margin-right: 4px;
}

.path-hint {
  margin-top: 16px;
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
