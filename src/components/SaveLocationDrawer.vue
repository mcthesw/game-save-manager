<script setup lang="ts">
import { $t } from "../i18n";
import type { SaveUnit, Device, Game } from "../bindings";
import { commands } from "../bindings";
import { useNotification } from "../composables/useNotification";
import { useConfig } from "../composables/useConfig";
import { ref, watch } from "vue";
import PathVariableSelector from "./PathVariableSelector.vue";

const { showSuccess, showError } = useNotification();

const props = defineProps({
    game: Object as () => Game,
})

const emits = defineEmits<{
    (event: 'closed'): void
    (event: 'saveChanges', game: Game): void
}>()

// 当前设备信息
const currentDevice = ref<Device | null>(null);
// 所有可用设备列表
const availableDevices = ref<Device[]>([]);
// 当前选中的设备ID
const selectedDeviceId = ref<string>('');
// 临时存储修改的数据
const tempGame = ref<Game>({ name: "", save_paths: [], game_paths: {} });
// 是否有未保存的修改
const hasUnsavedChanges = ref(false);

// 获取当前设备信息
async function fetchCurrentDevice() {
    try {
        const result = await commands.getCurrentDeviceInfo();
        if (result.status === "ok") {
            currentDevice.value = result.data;
            selectedDeviceId.value = currentDevice.value.id;
            console.log("Current device:", currentDevice.value);
        } else {
            showError({ message: result.error });
        }
    } catch (e) {
        console.error(`Error getting current device info:`, e);
        showError({ message: $t('error.get_device_info_failed') });
    }
}

// 从配置中获取所有设备信息
function getDevicesFromConfig() {
    // 从全局配置中获取设备信息
    const { config } = useConfig();
    
    // 将设备映射转换为Map以便快速查找
    const deviceMap = new Map<string, Device>();
    if (config.value && config.value.devices) {
        Object.entries(config.value.devices).forEach(([id, device]) => {
            if (device) { // 确保device不为undefined
                deviceMap.set(id, device);
            }
        });
    }
    
    return deviceMap;
}

// 从Game中提取所有设备ID
function extractDeviceIdsFromSaveUnits() {
    if (!props.game) return;

    // 收集所有设备ID
    const deviceIds = new Set<string>();

    // 添加当前设备ID
    if (currentDevice.value) {
        deviceIds.add(currentDevice.value.id);
    }

    // 从所有SaveUnit的paths中提取设备ID
    if (props.game.save_paths) {
        props.game.save_paths.forEach(unit => {
            if (unit.paths) {
                Object.keys(unit.paths).forEach(deviceId => {
                    deviceIds.add(deviceId);
                });
            }
        });
    }

    // 从game_paths中提取设备ID
    if (props.game.game_paths) {
        Object.keys(props.game.game_paths).forEach(deviceId => {
            deviceIds.add(deviceId);
        });
    }

    // 获取所有已知设备信息
    const deviceMap = getDevicesFromConfig();

    // 转换为设备对象数组
    availableDevices.value = Array.from(deviceIds).map(id => {
        // 如果是当前设备，使用当前设备信息
        if (currentDevice.value && id === currentDevice.value.id) {
            return currentDevice.value;
        } 
        // 如果在设备映射中找到，使用映射中的设备信息
        else if (deviceMap.has(id)) {
            return deviceMap.get(id)!;
        } 
        // 否则创建一个简单的设备对象
        else {
            return {
                id,
                name: id.substring(0, 8) + '...' // 截取ID的前8位作为名称
            };
        }
    });

    // 如果有当前设备，默认选择当前设备
    if (currentDevice.value) {
        selectedDeviceId.value = currentDevice.value.id;
    } else if (availableDevices.value.length > 0) {
        selectedDeviceId.value = availableDevices.value[0].id;
    }
}

// 监听game变化，重新提取设备ID并初始化临时数据
watch(() => props.game, () => {
    extractDeviceIdsFromSaveUnits();
    initTempGame();
}, { deep: true });

// 初始化设备信息
fetchCurrentDevice().then(() => {
    extractDeviceIdsFromSaveUnits();
    initTempGame();
});

// 初始化临时game数据
function initTempGame() {
    if (!props.game) return;
    // 深拷贝game数据
    tempGame.value = JSON.parse(JSON.stringify(props.game));
    hasUnsavedChanges.value = false;
}

// 获取当前设备的路径
function getDevicePath(unit: SaveUnit, deviceId: string): string {
    if (!unit.paths) return '';
    return unit.paths[deviceId] || '';
}

// 获取当前设备的游戏启动路径
function getGameLaunchPath(deviceId: string): string {
    if (!tempGame.value.game_paths) return '';
    return tempGame.value.game_paths[deviceId] || '';
}

// 更新临时设备路径
function updateDevicePath(index: number, deviceId: string, path: string) {
    if (!tempGame.value || !tempGame.value.save_paths) return;

    const unit = tempGame.value.save_paths[index];
    if (!unit.paths) {
        unit.paths = {};
    }

    unit.paths[deviceId] = path;
    hasUnsavedChanges.value = true;
}

// 更新临时游戏启动路径
function updateGameLaunchPath(deviceId: string, path: string) {
    if (!tempGame.value) return;

    if (!tempGame.value.game_paths) {
        tempGame.value.game_paths = {};
    }

    tempGame.value.game_paths[deviceId] = path;
    hasUnsavedChanges.value = true;
}

// 在路径输入框中插入变量
function insertPathVariable(variable: string, index: number, deviceId: string) {
    if (!tempGame.value || !tempGame.value.save_paths) return;

    const unit = tempGame.value.save_paths[index];
    if (!unit.paths) {
        unit.paths = {};
    }

    const currentPath = unit.paths[deviceId] || '';
    unit.paths[deviceId] = currentPath + variable;
    hasUnsavedChanges.value = true;
}

// 在游戏启动路径输入框中插入变量
function insertGamePathVariable(variable: string, deviceId: string) {
    if (!tempGame.value) return;

    if (!tempGame.value.game_paths) {
        tempGame.value.game_paths = {};
    }

    const currentPath = tempGame.value.game_paths[deviceId] || '';
    tempGame.value.game_paths[deviceId] = currentPath + variable;
    hasUnsavedChanges.value = true;
}

async function open(url: string) {
    let result = await commands.openFileOrFolder(url);
    if (result.status === "error") {
        showError({ message: $t("error.open_url_failed") });
    }
}

// 由父组件处理具体任务，此处只传递下标
function switch_delete_before_apply(unit: SaveUnit) {
    // 这里不需要特殊处理，直接修改tempGame中的值即可
    // 因为是引用类型，所以直接修改unit的属性会反映到tempGame中
    hasUnsavedChanges.value = true;
}

// 保存修改
function saveChanges() {
    if (!tempGame.value) return;

    // 将所有修改发送给父组件
    emits('saveChanges', tempGame.value);

    hasUnsavedChanges.value = false;
}

// 取消修改
function cancelChanges() {
    initTempGame();
}
</script>

<template>
    <el-drawer :title="$t('save_location_drawer.drawer_title')" size="70%" :on-closed="() => { $emit('closed') }">
        <!-- 操作按钮 -->
        <template #header>
            <div class="drawer-header">
                <span>{{ $t('save_location_drawer.drawer_title') }}</span>
                <div class="drawer-actions">
                    <el-button type="primary" size="small" @click="saveChanges" :disabled="!hasUnsavedChanges">
                        {{ $t('common.save') }}
                    </el-button>
                    <el-button size="small" @click="cancelChanges" :disabled="!hasUnsavedChanges">
                        {{ $t('common.cancel') }}
                    </el-button>
                </div>
            </div>
        </template>
        <!-- 设备选择器 -->
        <div class="device-selector">
            <el-select v-model="selectedDeviceId" :placeholder="$t('save_location_drawer.select_device')">
                <el-option v-for="device in availableDevices" :key="device.id" :label="device.name"
                    :value="device.id" />
            </el-select>
            <el-tag v-if="currentDevice && selectedDeviceId === currentDevice.id" type="success">
                {{ $t('save_location_drawer.current_device') }}
            </el-tag>
        </div>

        <!-- 游戏启动路径 -->
        <div class="launch-path-section">
            <h3>{{ $t('save_location_drawer.launch_path') }}</h3>
            <div class="path-input-container">
                <el-input :model-value="getGameLaunchPath(selectedDeviceId)" size="small"
                    @update:model-value="(value) => updateGameLaunchPath(selectedDeviceId, value)">
                    <template #append>
                        <div class="path-actions">
                            <path-variable-selector :current-path="getGameLaunchPath(selectedDeviceId)"
                                @insert="(variable) => insertGamePathVariable(variable, selectedDeviceId)" />
                        </div>
                    </template>
                </el-input>
            </div>
        </div>

        <!-- 存档路径表格 -->
        <h3>{{ $t('save_location_drawer.save_locations') }}</h3>
        <el-table :data="tempGame.save_paths" style="width: 100%" :border="true">
            <el-table-column prop="unit_type" :label="$t('save_location_drawer.type')" width="70" />
            <el-table-column :label="$t('save_location_drawer.prompt')" min-width="300">
                <template #default="scope">
                    <div class="path-input-container">
                        <el-input :model-value="getDevicePath(scope.row, selectedDeviceId)" size="small"
                            @update:model-value="(value) => updateDevicePath(scope.$index, selectedDeviceId, value)">
                            <template #append>
                                <div class="path-actions">
                                    <path-variable-selector :current-path="getDevicePath(scope.row, selectedDeviceId)"
                                        @insert="(variable) => insertPathVariable(variable, scope.$index, selectedDeviceId)" />
                                </div>
                            </template>
                        </el-input>
                    </div>
                </template>
            </el-table-column>
            <el-table-column prop="delete_before_apply" :label="$t('save_location_drawer.delete_before_apply')"
                width="100">
                <template #default="scope">
                    <el-switch v-model="scope.row.delete_before_apply"
                        @change="switch_delete_before_apply(scope.row)"></el-switch>
                </template>
            </el-table-column>
            <el-table-column :label="$t('save_location_drawer.open_file_header')" width="100">
                <template #default="scope">
                    <ElLink @click="open(getDevicePath(scope.row, selectedDeviceId))">
                        {{ $t('save_location_drawer.open') }}
                    </ElLink>
                </template>
            </el-table-column>
        </el-table>
    </el-drawer>
</template>

<style scoped>
.device-selector {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 20px;
}

.path-input-container {
    display: flex;
    align-items: center;
}

.path-actions {
    display: flex;
    gap: 5px;
}

.drawer-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    width: 100%;
}

.drawer-actions {
    display: flex;
    gap: 10px;
}
</style>