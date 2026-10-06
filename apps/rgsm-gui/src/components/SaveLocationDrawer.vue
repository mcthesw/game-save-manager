<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { error } from '../utils/logger';
import { $t } from '../i18n';
import type {
  Device,
  Game,
  GameDeviceBinding,
  OpenPathWarning,
  SaveUnit,
  SaveUnitType,
  SavePathOverride,
} from '../api/commands';
import { commands } from '../api/commands';
import { useConfig } from '../composables/useConfig';
import { useSaveLocationCheck } from '../composables/useSaveLocationCheck';
import PathVariableInput from './PathVariableInput.vue';
import SaveUnitPathInput from './SaveUnitPathInput.vue';
import PathVariablesEditor from './PathVariablesEditor.vue';
import { providePathVariableDraft, type VariableEdits } from '../composables/usePathVariableDraft';
import GameLocationSelection from './GameLocationSelection.vue';
import { saveUnitPaths, saveUnitType, escapePathLiteral } from '../utils/saveUnit';
import { hasGameNameConflict } from '../utils/gameName';
import { KButton, KDrawer, KInput, KSelect, KSwitch, KTag, KTooltip } from '../ui/kit';

const { config } = useConfig();
const feedback = useFeedback();
const { warnUnavailableLocations } = useSaveLocationCheck();

const props = defineProps<{
  game: Game;
  modelValue: boolean;
  continueOperation?: boolean;
}>();

const emits = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'closed'): void;
  (event: 'saveChanges', game: Game, variables: VariableEdits): void;
}>();

const open = computed({
  get: () => props.modelValue,
  set: (value: boolean) => emits('update:modelValue', value),
});

watch(open, (visible) => {
  if (!visible) emits('closed');
});

const currentDevice = ref<Device | null>(null);
const availableDevices = ref<Device[]>([]);
const selectedDeviceId = ref('');
const tempGame = ref<Game>({
  name: '',
  storage_key: '',
  save_paths: [],
  game_paths: {},
  device_bindings: {},
});
const hasUnsavedChanges = ref(false);

const selectedDevice = computed(() =>
  availableDevices.value.find((device) => device.id === selectedDeviceId.value)
);
const selectedBinding = computed(() => tempGame.value.device_bindings?.[selectedDeviceId.value]);
const variables = providePathVariableDraft(selectedDevice, selectedBinding);
watch(
  variables.edits,
  () => {
    hasUnsavedChanges.value = true;
  },
  { deep: true, flush: 'sync' }
);
const variablePaths = computed(() => [
  getGameLaunchPath(selectedDeviceId.value),
  ...tempGame.value.save_paths.map((unit) => getDevicePath(unit, selectedDeviceId.value)),
]);
const selectedDeviceResources = computed(() => selectedDevice.value?.resources ?? []);
const savedRestoreMappings = computed(
  () => tempGame.value.device_bindings?.[selectedDeviceId.value]?.restoreMappings ?? []
);

const deviceOptions = computed(() =>
  availableDevices.value.map((device) => ({ value: device.id, label: device.name }))
);

function currentBinding(): GameDeviceBinding {
  tempGame.value.device_bindings ??= {};
  return (tempGame.value.device_bindings[selectedDeviceId.value] ??= {
    restoreMappings: [],
  });
}

function updateBinding(binding: GameDeviceBinding) {
  tempGame.value.device_bindings ??= {};
  tempGame.value.device_bindings[selectedDeviceId.value] = binding;
  hasUnsavedChanges.value = true;
}

function removeRestoreMapping(index: number) {
  (currentBinding().restoreMappings ??= []).splice(index, 1);
  hasUnsavedChanges.value = true;
}

const activeSaveUnits = computed(() =>
  (tempGame.value.save_paths ?? []).filter((unit) => isUnitEnabled(unit))
);

const disabledSaveUnits = computed(() =>
  (tempGame.value.save_paths ?? []).filter((unit) => !isUnitEnabled(unit))
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
    Object.keys(saveUnitPaths(unit) ?? {}).forEach((deviceId) => deviceIds.add(deviceId));
  });

  Object.keys(game.game_paths ?? {}).forEach((deviceId) => deviceIds.add(deviceId));
  Object.keys(game.device_bindings ?? {}).forEach((deviceId) => deviceIds.add(deviceId));

  const deviceMap = getDevicesFromConfig();
  availableDevices.value = Array.from(deviceIds).map((id) => {
    if (currentDevice.value && id === currentDevice.value.id) {
      return deviceMap.get(id) ?? currentDevice.value;
    }

    return (
      deviceMap.get(id) ?? {
        id,
        name: `${id.substring(0, 8)}...`,
        resources: [],
        next_resource_id: 0,
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
  variables.reset();
  tempGame.value = JSON.parse(JSON.stringify(props.game));
  hasUnsavedChanges.value = false;
  syncAvailableDevices(tempGame.value);
}

watch(
  [() => props.modelValue, () => props.game.storage_key || props.game.name],
  ([visible]) => {
    // A background definition refresh must not replace an open editing session.
    if (visible) initTempGame();
  },
  { immediate: true }
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

function getOverride(unit: SaveUnit, deviceId = selectedDeviceId.value) {
  return unit.source.type === 'manifestPattern' && unit.id !== undefined
    ? tempGame.value.device_bindings?.[deviceId]?.pathOverrides?.[unit.id]
    : undefined;
}

function updatePathOverride(unit: SaveUnit, value: SavePathOverride | undefined) {
  if (unit.id === undefined) return;
  const binding = currentBinding();
  binding.pathOverrides ??= {};
  if (value) binding.pathOverrides[unit.id] = value;
  else delete binding.pathOverrides[unit.id];
  binding.restoreMappings = (binding.restoreMappings ?? []).filter(
    (rule) => rule.saveUnitId !== unit.id
  );
  hasUnsavedChanges.value = true;
}

function getDevicePath(unit: SaveUnit, deviceId: string): string {
  const override = getOverride(unit, deviceId);
  if (override) return override.expression;
  return unit.source.type === 'manifestPattern'
    ? unit.source.pattern
    : (unit.source.paths?.[deviceId] ?? '');
}

function getGameLaunchPath(deviceId: string): string {
  return tempGame.value.game_paths?.[deviceId] ?? '';
}

function updateDevicePath(unit: SaveUnit, deviceId: string, path: string) {
  const override = getOverride(unit, deviceId);
  if (override) {
    override.expression = path;
  } else if (unit.source.type === 'manifestPattern') {
    unit.source.pattern = path;
  } else {
    if (!deviceId) return;
    unit.source.paths ??= {};
    unit.source.paths[deviceId] = path;
  }
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

async function openManagedPath(path: string, pattern = false) {
  if (!path.trim()) {
    return;
  }

  try {
    const result = await commands.openFileOrFolder(
      path,
      tempGame.value,
      pattern,
      variables.edits.value
    );
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

async function saveChanges() {
  const trimmedName = tempGame.value.name.trim();
  if (!trimmedName) {
    notifyError($t('addgame.no_name_error'));
    return;
  }

  if (hasGameNameConflict(config.value.games, trimmedName, props.game)) {
    notifyError($t('addgame.duplicated_name_error'));
    return;
  }

  tempGame.value.name = trimmedName;
  if (currentDevice.value) {
    await warnUnavailableLocations(
      tempGame.value.save_paths,
      currentDevice.value.id,
      tempGame.value,
      variables.edits.value
    );
  }
  emits(
    'saveChanges',
    JSON.parse(JSON.stringify(tempGame.value)),
    JSON.parse(JSON.stringify(variables.edits.value))
  );
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
    return Object.values(saveUnitPaths(unit) ?? {}).includes(path);
  });

  if (duplicated) {
    notifyWarning($t('addgame.duplicated_path_error'));
    return false;
  }

  return true;
}

function createSaveUnit(unitType: SaveUnitType, path: string): SaveUnit {
  return {
    source: {
      type: 'devicePaths',
      unit_type: unitType,
      paths: selectedDeviceId.value ? { [selectedDeviceId.value]: path } : {},
    },
    delete_before_apply: config.value?.settings.default_delete_before_apply ?? false,
    enabled: true,
  };
}

function addSaveDirectory() {
  tempGame.value.save_paths.push(createSaveUnit('Folder', ''));
  hasUnsavedChanges.value = true;
}

function addSaveFile() {
  tempGame.value.save_paths.push(createSaveUnit('File', ''));
  hasUnsavedChanges.value = true;
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
  if (saveUnitType(unit) === 'WinRegistry') {
    const path = await promptRegistryPath(getDevicePath(unit, selectedDeviceId.value));
    if (!path || !checkSaveUnitUnique(path, unit)) {
      return;
    }

    updateDevicePath(unit, selectedDeviceId.value, path);
    return;
  }

  try {
    const result =
      saveUnitType(unit) === 'Folder'
        ? await commands.chooseSaveDir()
        : await commands.chooseSaveFile();

    if (result.status === 'error' || !checkSaveUnitUnique(result.data, unit)) {
      return;
    }

    updateDevicePath(unit, selectedDeviceId.value, escapePathLiteral(result.data));
  } catch (e) {
    error(`Error choosing save unit path: ${e}`);
    notifyError(
      saveUnitType(unit) === 'Folder'
        ? $t('error.choose_save_dir_error')
        : $t('error.choose_save_file_error')
    );
  }
}

function formatUnitType(unit: SaveUnit) {
  if (unit.source.type === 'manifestPattern' && !saveUnitType(unit)) {
    return $t('addgame.dynamic_path');
  }
  const unitType = saveUnitType(unit);
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
  if (saveUnitType(unit) === 'File') {
    return $t('save_location_drawer.open_ctrl_hint');
  }

  if (saveUnitType(unit) === 'WinRegistry') {
    return $t('save_location_drawer.registry_open_unsupported_title');
  }

  return $t('save_location_drawer.open');
}

async function handleOpenPath(e: MouseEvent, path: string, unit?: SaveUnit) {
  if (!path || !path.trim()) return;

  const ctrl = (e && (e as MouseEvent).ctrlKey) || (e && (e as MouseEvent).metaKey);
  if (ctrl && unit && saveUnitType(unit) === 'File') {
    const normalized = path.replace(/\\/g, '/');
    const idx = normalized.lastIndexOf('/');
    const parent = idx > -1 ? normalized.substring(0, idx) : normalized;
    await openManagedPath(parent, true);
    return;
  }

  await openManagedPath(path, !!unit);
}
</script>

<template>
  <KDrawer v-model:open="open" :width="720">
    <template #title>
      <span class="inline-flex items-center gap-1.5">
        {{ $t('save_location_drawer.drawer_title') }}
        <span v-if="hasUnsavedChanges" class="text-xs text-warning" aria-hidden="true">●</span>
      </span>
    </template>

    <div class="flex flex-col gap-3.5">
      <!-- Game name section -->
      <section>
        <div class="mb-1 block text-xs text-text-dim">{{ $t('addgame.game_name') }}</div>
        <KInput
          v-model="tempGame.name"
          class="w-full"
          :placeholder="$t('addgame.game_name')"
          :aria-label="$t('addgame.game_name')"
          @update:model-value="hasUnsavedChanges = true"
        />
      </section>

      <!-- Device selector section -->
      <section>
        <div class="mb-1 block text-xs text-text-dim">
          {{ $t('save_location_drawer.select_device') }}
        </div>
        <div class="flex items-center gap-2">
          <KSelect
            v-model="selectedDeviceId"
            class="min-w-56 flex-1"
            :options="deviceOptions"
            :placeholder="$t('save_location_drawer.select_device')"
            :aria-label="$t('save_location_drawer.select_device')"
          />
          <KTag
            v-if="currentDevice && selectedDeviceId === currentDevice.id"
            tone="success"
            class="shrink-0"
          >
            {{ $t('save_location_drawer.current_device') }}
          </KTag>
        </div>
      </section>

      <!-- Device resources section -->
      <section v-if="selectedDeviceResources.length">
        <GameLocationSelection
          :device="selectedDevice"
          :model-value="currentBinding()"
          :paths="[
            getGameLaunchPath(selectedDeviceId),
            ...tempGame.save_paths.flatMap((unit) => [
              ...(unit.source.type === 'manifestPattern' ? [unit.source.pattern] : []),
              getDevicePath(unit, selectedDeviceId),
            ]),
          ]"
          @update:model-value="updateBinding"
        />
        <div v-if="savedRestoreMappings.length" class="mt-2 flex flex-col gap-1">
          <div
            v-for="(mapping, index) in savedRestoreMappings"
            :key="`${mapping.saveUnitId}-${index}`"
            class="flex items-center justify-between gap-2 rounded-sm border border-border px-2.5 py-1.5"
          >
            <span class="text-xs text-text-dim">{{
              $t('save_location_drawer.saved_restore_choice', { id: mapping.saveUnitId })
            }}</span>
            <KButton
              variant="ghost"
              size="sm"
              class="text-danger"
              @click="removeRestoreMapping(index)"
            >
              {{ $t('save_location_drawer.forget_restore_choice') }}
            </KButton>
          </div>
        </div>
      </section>

      <!-- Launch path -->
      <section>
        <div class="mb-1.5 flex items-center justify-between gap-2">
          <div class="text-sm font-medium text-text">
            {{ $t('save_location_drawer.launch_path') }}
          </div>
          <div class="flex gap-1">
            <KButton
              variant="ghost"
              size="sm"
              :disabled="launcherPathEmpty"
              @click="openPath(getGameLaunchPath(selectedDeviceId))"
            >
              {{ $t('save_location_drawer.open') }}
            </KButton>
            <KButton variant="ghost" size="sm" @click="chooseLaunchPath">
              {{ $t('save_location_drawer.pick_path') }}
            </KButton>
          </div>
        </div>
        <PathVariableInput
          :game="tempGame"
          :model-value="getGameLaunchPath(selectedDeviceId)"
          :status-mode="selectedDeviceId === currentDevice?.id ? 'below' : 'none'"
          @update:model-value="updateGameLaunchPath(selectedDeviceId, String($event ?? ''))"
        />
      </section>

      <!-- Save locations toolbar -->
      <section>
        <div class="mb-1.5 flex flex-wrap items-center justify-between gap-2">
          <div class="text-sm font-medium text-text">
            {{ $t('save_location_drawer.save_locations') }}
          </div>
          <div class="flex flex-wrap gap-1.5">
            <KButton size="sm" @click="addSaveDirectory">
              {{ $t('addgame.add_save_directory') }}
            </KButton>
            <KButton size="sm" @click="addSaveFile">
              {{ $t('addgame.add_save_file') }}
            </KButton>
            <KButton size="sm" @click="addRegistryKey">
              {{ $t('addgame.add_registry_key') }}
            </KButton>
          </div>
        </div>
      </section>

      <!-- Active save units -->
      <div v-if="activeSaveUnits.length > 0" class="flex min-w-0 flex-col gap-6">
        <div
          v-for="(unit, index) in activeSaveUnits"
          :key="unit.id ?? `active-${index}`"
          class="flex min-w-0 flex-col gap-2"
        >
          <SaveUnitPathInput
            :unit="unit"
            :game="tempGame"
            :override-value="getOverride(unit)"
            :model-value="getDevicePath(unit, selectedDeviceId)"
            :local="selectedDeviceId === currentDevice?.id"
            @update:model-value="updateDevicePath(unit, selectedDeviceId, $event)"
            @update:override-value="updatePathOverride(unit, $event)"
          >
            <template #browse>
              <KButton variant="ghost" size="sm" @click="chooseUnitPath(unit)">
                {{ $t('save_location_drawer.pick_path') }}
              </KButton>
            </template>
            <template #type>
              <span class="text-xs font-medium text-text">{{ formatUnitType(unit) }}</span>
            </template>
            <template #identity>
              <span v-if="hasPersistentId(unit)" class="text-xs text-text-dim">#{{ unit.id }}</span>
              <KTag v-else tone="success">{{ $t('save_location_drawer.new_path') }}</KTag>
            </template>
            <template #actions>
              <KTooltip :content="openTooltip(unit)">
                <KButton
                  variant="ghost"
                  size="sm"
                  :disabled="!getDevicePath(unit, selectedDeviceId)"
                  @click="handleOpenPath($event, getDevicePath(unit, selectedDeviceId), unit)"
                >
                  {{ $t('save_location_drawer.open') }}
                </KButton>
              </KTooltip>
              <KButton
                v-if="!hasPersistentId(unit)"
                variant="ghost"
                size="sm"
                class="text-danger"
                @click="removeNewSaveUnit(unit)"
              >
                {{ $t('addgame.remove') }}
              </KButton>
            </template>
            <template #options>
              <label
                v-if="hasPersistentId(unit)"
                class="inline-flex cursor-pointer items-center gap-2 text-xs text-text-dim"
              >
                <span>{{ $t('save_location_drawer.backup_enabled') }}</span>
                <KSwitch
                  :model-value="isUnitEnabled(unit)"
                  @update:model-value="setUnitEnabled(unit, Boolean($event))"
                />
              </label>
              <label class="inline-flex cursor-pointer items-center gap-2 text-xs text-text-dim">
                <span>{{ $t('save_location_drawer.delete_before_apply') }}</span>
                <KSwitch
                  :model-value="unit.delete_before_apply"
                  @update:model-value="
                    unit.delete_before_apply = Boolean($event);
                    switchDeleteBeforeApply(unit);
                  "
                />
              </label>
            </template>
          </SaveUnitPathInput>
        </div>
      </div>

      <div v-else class="py-6 text-center text-sm text-text-dim">
        {{ $t('save_location_drawer.no_active_paths') }}
      </div>

      <p v-if="continueOperation" class="text-sm text-text-dim">
        {{ $t('path_variables.missing_hint') }}
      </p>
      <PathVariablesEditor
        v-if="selectedDevice && selectedDeviceId === currentDevice?.id"
        :device="selectedDevice"
        :binding="selectedBinding"
        :paths="variablePaths"
        :games="config.games"
        :game-key="tempGame.storage_key"
        @update:binding="updateBinding"
      />

      <!-- Disabled save units -->
      <template v-if="disabledSaveUnits.length > 0">
        <div class="flex items-center gap-2 text-xs font-medium text-text-dim">
          {{ $t('save_location_drawer.disabled_paths') }}
          <KTag>{{ disabledSaveUnits.length }}</KTag>
        </div>

        <div class="flex min-w-0 flex-col gap-6">
          <div
            v-for="(unit, index) in disabledSaveUnits"
            :key="unit.id ?? `disabled-${index}`"
            class="flex min-w-0 flex-col gap-2 opacity-70"
          >
            <SaveUnitPathInput
              :unit="unit"
              :game="tempGame"
              :override-value="getOverride(unit)"
              :model-value="getDevicePath(unit, selectedDeviceId)"
              :local="selectedDeviceId === currentDevice?.id"
              @update:model-value="updateDevicePath(unit, selectedDeviceId, $event)"
              @update:override-value="updatePathOverride(unit, $event)"
            >
              <template #browse>
                <KButton variant="ghost" size="sm" @click="chooseUnitPath(unit)">
                  {{ $t('save_location_drawer.pick_path') }}
                </KButton>
              </template>
              <template #type>
                <span class="text-xs font-medium text-text">{{ formatUnitType(unit) }}</span>
              </template>
              <template #identity>
                <span v-if="hasPersistentId(unit)" class="text-xs text-text-dim"
                  >#{{ unit.id }}</span
                >
                <KTag tone="warning">{{ $t('save_location_drawer.disabled') }}</KTag>
              </template>
              <template #actions>
                <KButton variant="ghost" size="sm" @click="chooseUnitPath(unit)">
                  {{ $t('save_location_drawer.pick_path') }}
                </KButton>
                <KTooltip :content="openTooltip(unit)">
                  <KButton
                    variant="ghost"
                    size="sm"
                    :disabled="!getDevicePath(unit, selectedDeviceId)"
                    @click="handleOpenPath($event, getDevicePath(unit, selectedDeviceId), unit)"
                  >
                    {{ $t('save_location_drawer.open') }}
                  </KButton>
                </KTooltip>
                <KButton size="sm" @click="setUnitEnabled(unit, true)">
                  {{ $t('save_location_drawer.restore') }}
                </KButton>
              </template>
            </SaveUnitPathInput>
          </div>
        </div>
      </template>
    </div>

    <template #footer>
      <KButton :disabled="!hasUnsavedChanges" @click="cancelChanges">
        {{ $t('common.cancel') }}
      </KButton>
      <KButton variant="primary" :disabled="!hasUnsavedChanges" @click="saveChanges">
        {{ $t(continueOperation ? 'path_variables.continue' : 'common.save') }}
      </KButton>
    </template>
  </KDrawer>
</template>
