<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { error } from '@tauri-apps/plugin-log';
import { $t } from '../i18n';
import type { Device, Game, OpenPathWarning, SaveUnit } from '../bindings';
import { commands } from '../bindings';
import { useConfig } from '../composables/useConfig';
import PathVariableInput from './PathVariableInput.vue';

const { config } = useConfig();
const feedback = useFeedback();

const props = defineProps({
  game: {
    type: Object as () => Game,
    required: true,
  },
});

const emits = defineEmits<{
  (event: 'closed'): void;
  (event: 'saveChanges', game: Game): void;
}>();

const currentDevice = ref<Device | null>(null);
const availableDevices = ref<Device[]>([]);
const selectedDeviceId = ref('');
const tempGame = ref<Game>({
  name: '',
  storage_key: '',
  save_paths: [],
  game_paths: {},
  store_user_ids: {},
});
const hasUnsavedChanges = ref(false);

const activeSaveUnits = computed(() =>
  (tempGame.value.save_paths ?? []).filter((unit) => isUnitEnabled(unit))
);

const disabledSaveUnits = computed(() =>
  (tempGame.value.save_paths ?? []).filter((unit) => !isUnitEnabled(unit))
);

const activePathsMissing = computed(
  () =>
    activeSaveUnits.value.length > 0 &&
    activeSaveUnits.value.every((unit) => getDevicePath(unit, selectedDeviceId.value).trim() === '')
);

const launcherPathEmpty = computed(() => getGameLaunchPath(selectedDeviceId.value).trim() === '');

function getDevicesFromConfig() {
  const deviceMap = new Map<string, Device>();

  if (config.value?.devices) {
    Object.entries(config.value.devices).forEach(([id, device]) => {
      if (device) {
        deviceMap.set(id, device);
      }
    });
  }

  return deviceMap;
}

function syncAvailableDevices(game: Game) {
  const deviceIds = new Set<string>();

  if (currentDevice.value) {
    deviceIds.add(currentDevice.value.id);
  }

  game.save_paths?.forEach((unit) => {
    Object.keys(unit.paths ?? {}).forEach((deviceId) => deviceIds.add(deviceId));
  });

  Object.keys(game.game_paths ?? {}).forEach((deviceId) => deviceIds.add(deviceId));

  const deviceMap = getDevicesFromConfig();
  availableDevices.value = Array.from(deviceIds).map((id) => {
    if (currentDevice.value && id === currentDevice.value.id) {
      return currentDevice.value;
    }

    return (
      deviceMap.get(id) ?? {
        id,
        name: `${id.substring(0, 8)}...`,
        game_roots: [],
      }
    );
  });

  if (
    currentDevice.value &&
    availableDevices.value.some((device) => device.id === currentDevice.value?.id)
  ) {
    if (
      !selectedDeviceId.value ||
      !availableDevices.value.some((device) => device.id === selectedDeviceId.value)
    ) {
      selectedDeviceId.value = currentDevice.value.id;
    }
    return;
  }

  if (!availableDevices.value.some((device) => device.id === selectedDeviceId.value)) {
    selectedDeviceId.value = availableDevices.value[0]?.id ?? '';
  }
}

function initTempGame() {
  tempGame.value = JSON.parse(JSON.stringify(props.game));
  hasUnsavedChanges.value = false;
  syncAvailableDevices(tempGame.value);
}

watch(
  () => props.game,
  () => {
    initTempGame();
  },
  { deep: true, immediate: true }
);

watch(
  () => currentDevice.value,
  () => {
    syncAvailableDevices(tempGame.value);
  }
);

async function fetchCurrentDevice() {
  try {
    const result = await commands.getCurrentDeviceInfo();
    if (result.status === 'ok') {
      currentDevice.value = result.data;
      if (!selectedDeviceId.value) {
        selectedDeviceId.value = result.data.id;
      }
      return;
    }

    notifyError(result.error);
  } catch (e) {
    error(`Error getting current device info: ${e}`);
    notifyError($t('error.get_device_info_failed'));
  }
}

void fetchCurrentDevice();

function getDevicePath(unit: SaveUnit, deviceId: string): string {
  return unit.paths?.[deviceId] ?? '';
}

function getGameLaunchPath(deviceId: string): string {
  return tempGame.value.game_paths?.[deviceId] ?? '';
}

function updateDevicePath(unit: SaveUnit, deviceId: string, path: string) {
  if (!deviceId) return;

  if (!unit.paths) {
    unit.paths = {};
  }

  unit.paths[deviceId] = path;
  hasUnsavedChanges.value = true;
}

function updateGameLaunchPath(deviceId: string, path: string) {
  if (!deviceId) return;

  if (!tempGame.value.game_paths) {
    tempGame.value.game_paths = {};
  }

  tempGame.value.game_paths[deviceId] = path;
  hasUnsavedChanges.value = true;
}

async function openPath(path: string) {
  await openManagedPath(path);
}

function notifyOpenPathWarning(warning: OpenPathWarning) {
  switch (warning) {
    case 'registryOpenUnsupported':
      notifyWarning(
        $t('save_location_drawer.registry_open_unsupported_title'),
        $t('save_location_drawer.registry_open_unsupported_detail')
      );
      return;
    default:
      notifyWarning($t('save_location_drawer.open_warning'));
  }
}

async function openManagedPath(path: string) {
  if (!path.trim()) {
    return;
  }

  try {
    const result = await commands.openFileOrFolder(path);
    if (result.status === 'error') {
      notifyError($t('error.open_path_failed'), result.error);
      return;
    }

    if (result.data.status === 'warning') {
      notifyOpenPathWarning(result.data.warning);
    }
  } catch (e) {
    error(`Error opening path: ${e}`);
    notifyError($t('error.open_path_failed'));
  }
}

function switchDeleteBeforeApply(_unit: SaveUnit) {
  hasUnsavedChanges.value = true;
}

function saveChanges() {
  const trimmedName = tempGame.value.name.trim();
  if (!trimmedName) {
    notifyError($t('addgame.no_name_error'));
    return;
  }

  const isDuplicate = config.value?.games.some(
    (g) =>
      g.storage_key !== props.game.storage_key && g.name.toLowerCase() === trimmedName.toLowerCase()
  );
  if (isDuplicate) {
    notifyError($t('addgame.duplicated_name_error'));
    return;
  }

  tempGame.value.name = trimmedName;
  emits('saveChanges', JSON.parse(JSON.stringify(tempGame.value)));
  hasUnsavedChanges.value = false;
}

function cancelChanges() {
  initTempGame();
}

function isUnitEnabled(unit: SaveUnit) {
  return unit.enabled !== false;
}

function hasPersistentId(unit: SaveUnit) {
  return typeof unit.id === 'number';
}

function setUnitEnabled(unit: SaveUnit, enabled: boolean) {
  unit.enabled = enabled;
  hasUnsavedChanges.value = true;
}

function removeNewSaveUnit(unit: SaveUnit) {
  const index = tempGame.value.save_paths.findIndex((candidate) => candidate === unit);
  if (index === -1) return;

  tempGame.value.save_paths.splice(index, 1);
  hasUnsavedChanges.value = true;
}

function checkSaveUnitUnique(path: string, ignoreUnit?: SaveUnit) {
  const duplicated = tempGame.value.save_paths.some((unit) => {
    if (unit === ignoreUnit) return false;
    return Object.values(unit.paths ?? {}).includes(path);
  });

  if (duplicated) {
    notifyWarning($t('addgame.duplicated_path_error'));
    return false;
  }

  return true;
}

function createSaveUnit(unitType: SaveUnit['unit_type'], path: string): SaveUnit {
  return {
    unit_type: unitType,
    paths: selectedDeviceId.value ? { [selectedDeviceId.value]: path } : {},
    delete_before_apply: config.value?.settings.default_delete_before_apply ?? false,
    enabled: true,
  };
}

async function addSaveDirectory() {
  try {
    const dir = await commands.chooseSaveDir();
    if (dir.status === 'error' || !checkSaveUnitUnique(dir.data)) {
      return;
    }

    tempGame.value.save_paths.push(createSaveUnit('Folder', dir.data));
    hasUnsavedChanges.value = true;
  } catch (e) {
    error(`Error choosing save directory: ${e}`);
    notifyError($t('error.choose_save_dir_error'));
  }
}

async function addSaveFile() {
  try {
    const file = await commands.chooseSaveFile();
    if (file.status === 'error' || !checkSaveUnitUnique(file.data)) {
      return;
    }

    tempGame.value.save_paths.push(createSaveUnit('File', file.data));
    hasUnsavedChanges.value = true;
  } catch (e) {
    error(`Error choosing save file: ${e}`);
    notifyError($t('error.choose_save_file_error'));
  }
}

async function validateRegistryPath(path: string) {
  const checkResult = await commands.checkPaths([path], null, null, null);
  if (checkResult.status !== 'ok') {
    return;
  }

  const [check] = checkResult.data;
  if (check && check.status === 'registryPath' && !check.supported) {
    notifyWarning($t('addgame.registry_non_windows_warning'));
  }
}

async function promptRegistryPath(initialValue = '') {
  try {
    const result = await feedback.prompt(
      $t('addgame.registry_key_prompt'),
      $t('addgame.add_registry_key'),
      {
        inputPlaceholder: 'HKEY_CURRENT_USER\\SOFTWARE\\GameName',
        inputValue: initialValue,
      }
    );

    return result.value?.trim() ?? '';
  } catch {
    return '';
  }
}

async function addRegistryKey() {
  const path = await promptRegistryPath();
  if (!path || !checkSaveUnitUnique(path)) {
    return;
  }

  await validateRegistryPath(path);
  tempGame.value.save_paths.push(createSaveUnit('WinRegistry', path));
  hasUnsavedChanges.value = true;
}

async function chooseLaunchPath() {
  try {
    const file = await commands.chooseSaveFile();
    if (file.status === 'error') {
      return;
    }

    updateGameLaunchPath(selectedDeviceId.value, file.data);
  } catch (e) {
    error(`Error choosing executable file: ${e}`);
    notifyError($t('error.choose_executable_file_error'));
  }
}

async function chooseUnitPath(unit: SaveUnit) {
  if (unit.unit_type === 'WinRegistry') {
    const path = await promptRegistryPath(getDevicePath(unit, selectedDeviceId.value));
    if (!path || !checkSaveUnitUnique(path, unit)) {
      return;
    }

    await validateRegistryPath(path);
    updateDevicePath(unit, selectedDeviceId.value, path);
    return;
  }

  try {
    const result =
      unit.unit_type === 'Folder'
        ? await commands.chooseSaveDir()
        : await commands.chooseSaveFile();

    if (result.status === 'error' || !checkSaveUnitUnique(result.data, unit)) {
      return;
    }

    updateDevicePath(unit, selectedDeviceId.value, result.data);
  } catch (e) {
    error(`Error choosing save unit path: ${e}`);
    notifyError(
      unit.unit_type === 'Folder'
        ? $t('error.choose_save_dir_error')
        : $t('error.choose_save_file_error')
    );
  }
}

function formatUnitType(unitType: SaveUnit['unit_type']) {
  switch (unitType) {
    case 'Folder':
      return $t('save_location_drawer.type_folder');
    case 'File':
      return $t('save_location_drawer.type_file');
    case 'WinRegistry':
      return $t('save_location_drawer.type_registry');
    default:
      return unitType;
  }
}

function openTooltip(unit: SaveUnit) {
  if (unit.unit_type === 'File') {
    return $t('save_location_drawer.open_ctrl_hint');
  }

  if (unit.unit_type === 'WinRegistry') {
    return $t('save_location_drawer.registry_open_unsupported_title');
  }

  return $t('save_location_drawer.open');
}

async function handleOpenPath(e: MouseEvent, path: string, unit?: SaveUnit) {
  if (!path || !path.trim()) return;

  const ctrl = (e && (e as MouseEvent).ctrlKey) || (e && (e as MouseEvent).metaKey);
  if (ctrl && unit && unit.unit_type === 'File') {
    const normalized = path.replace(/\\/g, '/');
    const idx = normalized.lastIndexOf('/');
    const parent = idx > -1 ? normalized.substring(0, idx) : normalized;
    await openManagedPath(parent);
    return;
  }

  await openManagedPath(path);
}
</script>

<template>
  <el-drawer
    :title="$t('save_location_drawer.drawer_title')"
    size="70%"
    :on-closed="
      () => {
        $emit('closed');
      }
    "
  >
    <template #header>
      <div class="drawer-header">
        <span class="drawer-title">
          {{ $t('save_location_drawer.drawer_title') }}
          <span v-if="hasUnsavedChanges" class="unsaved-indicator">●</span>
        </span>
        <div class="drawer-actions">
          <el-button
            type="primary"
            size="small"
            :disabled="!hasUnsavedChanges"
            @click="saveChanges"
          >
            {{ $t('common.save') }}
          </el-button>
          <el-button size="small" :disabled="!hasUnsavedChanges" @click="cancelChanges">
            {{ $t('common.cancel') }}
          </el-button>
        </div>
      </div>
    </template>

    <div class="drawer-body">
      <!-- Game name section -->
      <div class="section">
        <div class="section-header">
          <div class="section-label">{{ $t('addgame.game_name') }}</div>
        </div>
        <el-input
          v-model="tempGame.name"
          :placeholder="$t('addgame.game_name')"
          @input="hasUnsavedChanges = true"
        />
      </div>

      <!-- Device selector section -->
      <div class="section">
        <div class="section-header">
          <div class="section-label">{{ $t('save_location_drawer.select_device') }}</div>
        </div>
        <div class="device-selector-row">
          <el-select
            v-model="selectedDeviceId"
            :placeholder="$t('save_location_drawer.select_device')"
            style="flex: 1; min-width: 220px"
          >
            <el-option
              v-for="device in availableDevices"
              :key="device.id"
              :label="device.name"
              :value="device.id"
            />
          </el-select>
          <el-tag
            v-if="currentDevice && selectedDeviceId === currentDevice.id"
            type="success"
            style="flex-shrink: 0"
          >
            {{ $t('save_location_drawer.current_device') }}
          </el-tag>
        </div>
      </div>

      <!-- Launch path -->
      <div class="section">
        <div class="section-header">
          <div class="section-label">{{ $t('save_location_drawer.launch_path') }}</div>
          <div class="section-actions">
            <el-button
              text
              size="small"
              :disabled="launcherPathEmpty"
              @click="openPath(getGameLaunchPath(selectedDeviceId))"
            >
              {{ $t('save_location_drawer.open') }}
            </el-button>
            <el-button type="primary" text size="small" @click="chooseLaunchPath">
              {{ $t('save_location_drawer.pick_path') }}
            </el-button>
          </div>
        </div>
        <path-variable-input
          :model-value="getGameLaunchPath(selectedDeviceId)"
          status-mode="below"
          @update:model-value="(value) => updateGameLaunchPath(selectedDeviceId, value)"
        />
      </div>

      <!-- Save locations toolbar -->
      <div class="section">
        <div class="section-header">
          <div class="section-label">{{ $t('save_location_drawer.save_locations') }}</div>
          <div class="section-actions">
            <el-button type="primary" size="small" @click="addSaveDirectory">
              {{ $t('addgame.add_save_directory') }}
            </el-button>
            <el-button size="small" @click="addSaveFile">
              {{ $t('addgame.add_save_file') }}
            </el-button>
            <el-button size="small" @click="addRegistryKey">
              {{ $t('addgame.add_registry_key') }}
            </el-button>
          </div>
        </div>

        <el-alert
          v-if="activePathsMissing"
          type="info"
          show-icon
          :closable="false"
          class="section-alert"
        >
          {{ $t('save_location_drawer.device_paths_empty') }}
        </el-alert>
      </div>

      <!-- Active save units -->
      <div v-if="activeSaveUnits.length > 0" class="unit-list">
        <div
          v-for="(unit, index) in activeSaveUnits"
          :key="unit.id ?? `active-${index}`"
          class="unit-card"
        >
          <div class="unit-card-top">
            <div class="unit-card-tags">
              <el-tag size="small">{{ formatUnitType(unit.unit_type) }}</el-tag>
              <el-tag v-if="hasPersistentId(unit)" type="info" size="small">#{{ unit.id }}</el-tag>
              <el-tag v-else type="success" size="small">{{
                $t('save_location_drawer.new_path')
              }}</el-tag>
            </div>
            <div class="unit-card-actions">
              <el-tooltip :content="openTooltip(unit)" placement="top">
                <el-button
                  text
                  size="small"
                  :disabled="!getDevicePath(unit, selectedDeviceId)"
                  @click="
                    (e: MouseEvent) =>
                      handleOpenPath(e, getDevicePath(unit, selectedDeviceId), unit)
                  "
                >
                  {{ $t('save_location_drawer.open') }}
                </el-button>
              </el-tooltip>
              <el-button text size="small" @click="chooseUnitPath(unit)">
                {{ $t('save_location_drawer.pick_path') }}
              </el-button>
              <el-button
                v-if="!hasPersistentId(unit)"
                text
                size="small"
                type="danger"
                @click="removeNewSaveUnit(unit)"
              >
                {{ $t('addgame.remove') }}
              </el-button>
            </div>
          </div>

          <path-variable-input
            :model-value="getDevicePath(unit, selectedDeviceId)"
            status-mode="tooltip"
            @update:model-value="(value: string) => updateDevicePath(unit, selectedDeviceId, value)"
          />

          <div v-if="!getDevicePath(unit, selectedDeviceId).trim()" class="path-hint">
            {{ $t('save_location_drawer.path_missing_for_device') }}
          </div>

          <div class="toggle-row">
            <label v-if="hasPersistentId(unit)" class="toggle-item">
              <span>{{ $t('save_location_drawer.backup_enabled') }}</span>
              <el-switch
                :model-value="isUnitEnabled(unit)"
                @change="(value: string | number | boolean) => setUnitEnabled(unit, Boolean(value))"
              />
            </label>
            <label class="toggle-item">
              <span>{{ $t('save_location_drawer.delete_before_apply') }}</span>
              <el-switch
                v-model="unit.delete_before_apply"
                @change="switchDeleteBeforeApply(unit)"
              />
            </label>
          </div>
        </div>
      </div>

      <div v-else class="empty-state">
        {{ $t('save_location_drawer.no_active_paths') }}
      </div>

      <!-- Disabled save units -->
      <template v-if="disabledSaveUnits.length > 0">
        <div class="section-label disabled-section-heading">
          {{ $t('save_location_drawer.disabled_paths') }}
          <el-tag type="info" size="small" class="disabled-count">{{
            disabledSaveUnits.length
          }}</el-tag>
        </div>

        <div class="unit-list">
          <div
            v-for="(unit, index) in disabledSaveUnits"
            :key="unit.id ?? `disabled-${index}`"
            class="unit-card unit-card--disabled"
          >
            <div class="unit-card-top">
              <div class="unit-card-tags">
                <el-tag type="info" size="small">{{ formatUnitType(unit.unit_type) }}</el-tag>
                <el-tag v-if="hasPersistentId(unit)" type="info" size="small"
                  >#{{ unit.id }}</el-tag
                >
                <el-tag type="warning" size="small">{{
                  $t('save_location_drawer.disabled')
                }}</el-tag>
              </div>
              <div class="unit-card-actions">
                <el-button text size="small" @click="chooseUnitPath(unit)">
                  {{ $t('save_location_drawer.pick_path') }}
                </el-button>
                <el-tooltip :content="openTooltip(unit)" placement="top">
                  <el-button
                    text
                    size="small"
                    :disabled="!getDevicePath(unit, selectedDeviceId)"
                    @click="
                      (e: MouseEvent) =>
                        handleOpenPath(e, getDevicePath(unit, selectedDeviceId), unit)
                    "
                  >
                    {{ $t('save_location_drawer.open') }}
                  </el-button>
                </el-tooltip>
                <el-button type="primary" size="small" plain @click="setUnitEnabled(unit, true)">
                  {{ $t('save_location_drawer.restore') }}
                </el-button>
              </div>
            </div>

            <path-variable-input
              :model-value="getDevicePath(unit, selectedDeviceId)"
              status-mode="tooltip"
              @update:model-value="(value) => updateDevicePath(unit, selectedDeviceId, value)"
            />

            <div v-if="!getDevicePath(unit, selectedDeviceId).trim()" class="path-hint">
              {{ $t('save_location_drawer.path_missing_for_device') }}
            </div>
          </div>
        </div>
      </template>
    </div>
  </el-drawer>
</template>

<style scoped>
.drawer-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px 12px;
  width: 100%;
}

.drawer-title {
  font-size: 20px;
  font-weight: 600;
  color: var(--el-text-color-primary);
  display: flex;
  align-items: center;
  line-height: 1.3;
  min-width: 0;
}

.unsaved-indicator {
  color: var(--el-color-warning);
  font-size: 11px;
  margin-left: 6px;
  line-height: 1;
}

.drawer-actions {
  display: flex;
  gap: 10px;
}

.drawer-body {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.device-selector-row {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.section {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.section-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 8px;
}

.section-label {
  color: var(--el-text-color-secondary);
  border-left: 3px solid var(--el-color-primary-light-5);
  padding-left: 8px;
  font-size: 14px;
  font-weight: bold;
}

.section-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.section-alert {
  margin-bottom: 12px;
}

.unit-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.unit-card {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 14px;
  border: 1px solid var(--el-border-color-light);
  border-radius: 8px;
  background: var(--el-bg-color);
}

.unit-card--disabled {
  border-style: dashed;
  opacity: 0.72;
}

.unit-card-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-wrap: wrap;
}

.unit-card-tags {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}

.unit-card-actions {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}

.path-hint {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}

.toggle-row {
  display: flex;
  gap: 16px;
  flex-wrap: wrap;
}

.toggle-item {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--el-text-color-regular);
}

.empty-state {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 80px;
  border: 1px dashed var(--el-border-color-light);
  border-radius: 8px;
  color: var(--el-text-color-secondary);
  font-size: 13px;
}

.disabled-section-heading {
  margin-top: 8px;
}

.disabled-count {
  margin-left: 8px;
}
</style>
