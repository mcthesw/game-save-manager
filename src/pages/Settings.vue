<script lang="ts" setup>
// TODO:调整日志设置，比如删除日
import { computed, ref, watch, onMounted } from "vue";
import { $t, i18n } from "../i18n";
import { ElMessageBox, ElOption } from "element-plus";
import { useI18n } from "vue-i18n";
import draggable from 'vuedraggable'
import { DocumentAdd, HotWater, InfoFilled, MostlyCloudy, Setting, SwitchFilled, Document, Unlock, Moon, Tools } from "@element-plus/icons-vue";
import HotkeySelector from "../components/HotkeySelector.vue";
import { useDark } from '@vueuse/core'
import { invoke } from "@tauri-apps/api/core";
import { commands } from "~/bindings";
import { error, info } from "@tauri-apps/plugin-log";
import type { Device } from "../bindings";

const isDark = useDark()
const { config, refreshConfig, saveConfig } = useConfig()
const { showSuccess, showError, showInfo } = useNotification()
const locale_message = i18n.global.messages
const locale_names = i18n.global.availableLocales
const activeTab = ref('general')
const hotkeysChanged = ref(false)
const gameOrderChanged = ref(false)
const { withLoading } = useGlobalLoading()

type PreviewSoundResponse =
    | { status: "started"; fallback_error?: string | null }
    | { status: "stopped" }

const successSoundCache = ref("")
const failureSoundCache = ref("")
const activePreviewTarget = ref<null | 'success' | 'failure'>(null)

const successPreviewButtonText = computed(() =>
    activePreviewTarget.value === 'success'
        ? $t('settings.quick_action_sound_stop_preview')
        : $t('settings.quick_action_sound_preview')
)

const failurePreviewButtonText = computed(() =>
    activePreviewTarget.value === 'failure'
        ? $t('settings.quick_action_sound_stop_preview')
        : $t('settings.quick_action_sound_preview')
)

watch(
    () => config.value.quick_action?.sound.enabled,
    (enabled) => {
        if (enabled === false) {
            activePreviewTarget.value = null
        }
    }
)

const quickActionSuccessSoundMode = computed<'default' | 'custom'>({
    get: () => config.value.quick_action?.sound.success.type ?? 'default',
    set: (mode) => {
        if (!config.value?.quick_action) { return }
        const sound = config.value.quick_action.sound
        if (mode === 'default') {
            if (sound.success.type === 'custom') {
                successSoundCache.value = sound.success.path ?? ''
            }
            sound.success = { type: 'default' }
        } else {
            const cachedPath = sound.success.type === 'custom'
                ? sound.success.path ?? ''
                : successSoundCache.value
            sound.success = { type: 'custom', path: cachedPath }
        }
    }
})

const quickActionSuccessSoundPath = computed({
    get: () => {
        const variant = config.value.quick_action?.sound.success
        if (!variant) { return successSoundCache.value }
        if (variant.type === 'custom') {
            return variant.path ?? ''
        }
        return successSoundCache.value
    },
    set: (value: string) => {
        if (!config.value?.quick_action) { return }
        successSoundCache.value = value
        config.value.quick_action.sound.success = { type: 'custom', path: value }
    }
})

const quickActionFailureSoundMode = computed<'default' | 'custom'>({
    get: () => config.value.quick_action?.sound.failure.type ?? 'default',
    set: (mode) => {
        if (!config.value?.quick_action) { return }
        const sound = config.value.quick_action.sound
        if (mode === 'default') {
            if (sound.failure.type === 'custom') {
                failureSoundCache.value = sound.failure.path ?? ''
            }
            sound.failure = { type: 'default' }
        } else {
            const cachedPath = sound.failure.type === 'custom'
                ? sound.failure.path ?? ''
                : failureSoundCache.value
            sound.failure = { type: 'custom', path: cachedPath }
        }
    }
})

const quickActionFailureSoundPath = computed({
    get: () => {
        const variant = config.value.quick_action?.sound.failure
        if (!variant) { return failureSoundCache.value }
        if (variant.type === 'custom') {
            return variant.path ?? ''
        }
        return failureSoundCache.value
    },
    set: (value: string) => {
        if (!config.value?.quick_action) { return }
        failureSoundCache.value = value
        config.value.quick_action.sound.failure = { type: 'custom', path: value }
    }
})

// 设备管理相关
const currentDevice = ref<Device>({ id: "", name: "" })
const otherDevices = ref<Device[]>([])
const deviceNameChanged = ref(false)

// 使用debounce来合并多次保存操作
const debouncedSaveConfig = useDebounceFn(async () => {
    try {
        await saveConfig();
    } catch (e) {
        error(`save config error: ${e}`)
        showError({ message: $t("error.set_config_failed") })
    }
}, 500)

async function load_config() {
    await refreshConfig()
    await fetchDeviceInfo()
}

async function reset_settings() {
    try {
        await commands.resetSettings()
        showSuccess({ message: $t("settings.reset_success") });
        load_config();
    } catch (e) {
        error(`reset settings error: ${e}`)
        showError({ message: $t("error.reset_settings_failed") })
    }
}

async function backup_all() {
    try {
        await ElMessageBox.prompt(
            $t('settings.backup_all_hint'),
            $t('home.hint'),
            {
                confirmButtonText: $t('settings.confirm'),
                cancelButtonText: $t('settings.cancel'),
                inputPattern: /yes/,
                inputErrorMessage: $t('settings.invalid_input_error'),
            }
        );

        try {
            await withLoading(async () => {
                await commands.backupAll();
            }, $t('settings.backup_all_in_progress'));
            showSuccess({ message: $t("settings.success") });
        } catch (e) {
            error(`backup all error: ${e}`)
            showError({ message: $t("settings.failed") });
        }
    } catch {
        showInfo({ message: $t('settings.operation_canceled') });
    }
}

async function apply_all() {
    try {
        await ElMessageBox.prompt(
            $t('settings.apply_all_hint'),
            $t('home.hint'),
            {
                confirmButtonText: $t('settings.confirm'),
                cancelButtonText: $t('settings.cancel'),
                inputPattern: /yes/,
                inputErrorMessage: $t('settings.invalid_input_error'),
            }
        );
        await withLoading(async () => {
            await commands.applyAll();
        }, $t('settings.apply_all_in_progress'));
    } catch (e) {
        if (e instanceof Error) {
            error(`apply all error: ${e}`);
        } else {
            showInfo({ message: $t('settings.operation_canceled') });
        }
    }
}

function open_log_folder() {
    try {
        commands.openFileOrFolder("log")
    } catch (e) {
        error(`open log folder error: ${e}`)
        showError({ message: $t('error.open_log_folder_failed') })
    }
}

async function chooseQuickActionSound(target: 'success' | 'failure') {
    try {
        const result = await commands.chooseAudioFile()
        if (result.status === 'error') { return }
        if (target === 'success') {
            quickActionSuccessSoundPath.value = result.data
        } else {
            quickActionFailureSoundPath.value = result.data
        }
    } catch (e) {
        error(`choose audio file error: ${e}`)
        showError({ message: $t('error.choose_audio_file_error') })
    }
}

async function previewQuickActionSound(target: 'success' | 'failure') {
    try {
        if (typeof debouncedSaveConfig.flush === 'function') {
            await debouncedSaveConfig.flush()
        }

        const quickAction = config.value.quick_action
        if (!quickAction) { return }

        const variant = target === 'success'
            ? quickAction.sound.success
            : quickAction.sound.failure

        const response = await invoke<PreviewSoundResponse>('preview_quick_action_sound', {
            kind: target,
            variant,
        })
        if (response.status === 'started') {
            activePreviewTarget.value = target
            if (response.fallback_error) {
                const fallbackMessage = $t('error.preview_audio_file_error')
                const message = `${fallbackMessage}\n${response.fallback_error}`
                showError({ message })
            }
        } else {
            activePreviewTarget.value = null
        }
    } catch (e) {
        activePreviewTarget.value = null
        const fallbackMessage = $t('error.preview_audio_file_error')
        const detail = e instanceof Error ? e.message : typeof e === 'string' ? e : ''
        error(`preview quick action sound error: ${detail || String(e)}`)
        const message = detail ? `${fallbackMessage}\n${detail}` : fallbackMessage
        showError({ message })
    }
}

// 保存快捷键设置
async function saveHotkeys() {
    try {
        await saveConfig();
        hotkeysChanged.value = false;
        // 只显示功能完成的消息，而不是保存成功
        showSuccess({ message: $t("settings.hotkeys_saved") });
    } catch (e) {
        error(`save hotkeys error: ${e}`)
        showError({ message: $t("error.set_config_failed") })
    }
}

// 保存游戏顺序设置
async function saveGameOrder() {
    try {
        await saveConfig();
        gameOrderChanged.value = false;
        // 只显示功能完成的消息，而不是保存成功
        showSuccess({ message: $t("settings.game_order_saved") });
    } catch (e) {
        error(`save game order error: ${e}`)
        showError({ message: $t("error.set_config_failed") })
    }
}

// 翻译网站
async function translate_website() {
    try {
        await commands.openUrl("https://github.com/mcthesw/game-save-manager/blob/main/CONTRIBUTING.md")
    } catch (e) {
        error(`open translate website error: ${e}`)
        showError({ message: $t('error.open_url_failed') })
    }
}

// 获取设备信息
async function fetchDeviceInfo() {
    try {
        // 获取当前设备信息
        const result = await commands.getCurrentDeviceInfo();
        if (result.status === "ok") {
            currentDevice.value = result.data;
            
            // 从配置中获取所有设备
            if (config.value && config.value.devices) {
                // 过滤掉当前设备，只显示其他设备
                otherDevices.value = Object.values(config.value.devices)
                    .filter(device => device && device.id !== currentDevice.value.id)
                    // 确保过滤后的数组不包含undefined
                    .filter((device): device is Device => device !== undefined);
            }
        } else {
            showError({ message: result.error });
        }
    } catch (e) {
        error(`Error getting device info: ${e}`);
        showError({ message: $t('error.get_device_info_failed') });
    }
}

// 更新设备信息
async function updateDeviceInfo() {
    try {
        if (!config.value || !currentDevice.value) return;
        
        // 在配置中更新设备信息
        if (!config.value.devices) {
            config.value.devices = {};
        }
        
        config.value.devices[currentDevice.value.id] = { ...currentDevice.value };
        
        // 保存配置
        await saveConfig();
        showSuccess({ message: $t('settings.device_updated') });
        await fetchDeviceInfo(); // 刷新设备列表
    } catch (e) {
        error(`Error updating device info: ${e}`);
        showError({ message: $t('error.update_device_failed') });
    }
}

// 从其他设备导入路径
async function importFromDevice(deviceId: string) {
    try {
        await ElMessageBox.confirm(
            $t('settings.import_paths_confirm'),
            $t('settings.import_paths_title'),
            {
                confirmButtonText: $t('settings.confirm'),
                cancelButtonText: $t('settings.cancel'),
                type: 'warning',
            }
        );
        
        // 获取当前设备ID
        const currentDeviceId = currentDevice.value?.id;
        if (!currentDeviceId || !config.value || !config.value.games) {
            throw new Error("Current device or config not available");
        }
        
        if (currentDeviceId === deviceId) {
            throw new Error("Cannot import from the same device");
        }
        
        // 遍历所有游戏，复制源设备的路径到当前设备
        for (const game of config.value.games) {
            // 复制存档路径
            for (const savePath of game.save_paths || []) {
                if (savePath.paths) {
                    if (savePath.paths[deviceId]) {
                        savePath.paths[currentDeviceId] = savePath.paths[deviceId];
                    }
                }
            }
            
            // 复制游戏启动路径
            if (game.game_paths && game.game_paths[deviceId]) {
                game.game_paths[currentDeviceId] = game.game_paths[deviceId];
            }
        }
        
        // 保存配置
        await saveConfig();
        showSuccess({ message: $t('settings.import_paths_success') });
    } catch (e) {
        if (e instanceof Error) {
            error(`Error importing paths: ${e}`);
            showError({ message: $t('error.import_paths_failed') });
        } else {
            // 用户取消操作
            showInfo({ message: $t('settings.operation_canceled') });
        }
    }
}

// 删除设备
async function deleteDevice(deviceId: string) {
    if (!config.value || !config.value.devices) {
        showError({ message: $t('settings.delete_device_failed') });
        return;
    }

    if (currentDevice.value?.id === deviceId) {
        showError({ message: $t('settings.delete_device_failed') });
        return;
    }

    const targetDevice = config.value.devices[deviceId];
    if (!targetDevice) {
        showError({ message: $t('settings.delete_device_failed') });
        return;
    }

    try {
        await ElMessageBox.confirm(
            `${$t('settings.delete_device_confirm_message')}

${$t('settings.device_name')}: ${targetDevice.name || deviceId}`,
            $t('settings.delete_device_confirm_title'),
            {
                confirmButtonText: $t('settings.confirm'),
                cancelButtonText: $t('settings.cancel'),
                type: 'warning',
            }
        );
    } catch {
        showInfo({ message: $t('settings.operation_canceled') });
        return;
    }

    try {
        delete config.value.devices[deviceId];

        if (Array.isArray(config.value.games)) {
            for (const game of config.value.games) {
                if (game.game_paths && deviceId in game.game_paths) {
                    delete game.game_paths[deviceId];
                }

                for (const saveUnit of game.save_paths || []) {
                    if (saveUnit.paths && deviceId in saveUnit.paths) {
                        delete saveUnit.paths[deviceId];
                    }
                }
            }
        }

        await saveConfig();
        showSuccess({ message: $t('settings.delete_device_success') });
        await fetchDeviceInfo();
    } catch (e) {
        error(`Error deleting device ${deviceId}: ${e}`);
        await refreshConfig();
        showError({ message: $t('settings.delete_device_failed') });
    }
}

watch(
    () => config.value.quick_action?.sound.success,
    (variant) => {
        if (variant && variant.type === 'custom') {
            successSoundCache.value = variant.path ?? ''
        }
    },
    { immediate: true, deep: true }
)

watch(
    () => config.value.quick_action?.sound.failure,
    (variant) => {
        if (variant && variant.type === 'custom') {
            failureSoundCache.value = variant.path ?? ''
        }
    },
    { immediate: true, deep: true }
)

watch(
    () => config.value.quick_action?.sound,
    () => {
        debouncedSaveConfig()
    },
    { deep: true }
)

watch(
    () => config.value.quick_action?.notifications,
    () => {
        debouncedSaveConfig()
    },
    { deep: true }
)

// 监听快捷键变更
watch(
    () => config.value.quick_action?.hotkeys,
    () => {
        hotkeysChanged.value = true;
    },
    { deep: true }
)

// 监听游戏顺序变更
watch(
    () => config.value.games,
    () => {
        gameOrderChanged.value = true;
    },
    { deep: true }
)

// 页面加载时获取设备信息
onMounted(async () => {
    await fetchDeviceInfo();
})

watch(
    () => config.value.settings.locale,
    (new_locale, _old_locale) => {
        info(`locale changed to ${new_locale}`)
        if (new_locale)
            i18n.global.locale.value = new_locale as any;
        showInfo({ message: $t("settings.locale_changed") });
    }
)

watch(
    () => config.value?.settings,
    async () => {
        debouncedSaveConfig();
    },
    { deep: true } // 深度监听对象变化
)

const router_list = computed(() => {
    // TODO:抽离到新文件中，同时`MainSideBar.vue`也要抽离
    var link_list = [
        { text: $t("sidebar.homepage"), link: "/", icon: HotWater },
        { text: $t("sidebar.add_game"), link: "/AddGame", icon: DocumentAdd },
        { text: $t("sidebar.sync_settings"), link: "/SyncSettings", icon: MostlyCloudy },
        { text: $t("sidebar.settings"), link: "/Settings", icon: Setting },
        { text: $t("sidebar.about"), link: "/About", icon: InfoFilled },
    ]
    config.value?.games.forEach((game) => {
        link_list.push({ text: game.name, link: `/Management/${game.name}`, icon: SwitchFilled })
    })
    return link_list
})
</script>

<template>
    <el-container class="setting" direction="vertical">
        <el-card>
            <h1>{{ $t("settings.customizable_settings") }}</h1>
            <div class="button-bar">
                <el-button @click="open_log_folder()">{{ $t("settings.open_log_folder") }}</el-button>
                <el-popconfirm :title="$t('settings.confirm_reset')" :on-confirm="reset_settings">
                    <template #reference>
                        <el-button type="danger">{{ $t("settings.reset_settings") }}</el-button>
                    </template>
                </el-popconfirm>
                <el-button @click="backup_all" type="danger">
                    {{ $t("settings.backup_all") }}
                </el-button>
                <el-button @click="apply_all" type="danger">
                    {{ $t("settings.apply_all") }}
                </el-button>
            </div>

            <el-tabs v-model="activeTab" type="border-card" class="settings-tabs">
                <!-- 通用设置 -->
                <el-tab-pane :label="$t('settings.general')" name="general">
                    <el-divider content-position="left">
                        <el-icon>
                            <Setting />
                        </el-icon>
                        <span class="tab-title">{{ $t('settings.general') }}</span>
                    </el-divider>

                    <div class="setting-box">
                        <ElSelect v-model="config.settings.locale">
                            <ElOption v-for="locale in locale_names" :key="locale"
                                :label="(locale_message[locale])['settings']['locale_name'] + ' - ' + locale"
                                :value="locale" />
                        </ElSelect>
                        <span class="setting-label translate-website" @click="translate_website">🌍
                            Languages - Click me to translate!</span>
                    </div>
                    <div class="setting-box">
                        <ElSelect v-model="config.settings.home_page">
                            <ElOption v-for="route_info in router_list" :key="route_info.text" :label="route_info.text"
                                :value="route_info.link">
                                <div class="home-option-box">
                                    <component :is="route_info.icon" class="home-box-icon"></component>
                                    {{ route_info.text }}
                                </div>
                            </ElOption>
                        </ElSelect>
                        <span class="setting-label">🏠 {{ $t("settings.homepage") }}</span>
                    </div>
                    <div class="setting-box">
                        <ElSwitch v-model="config.settings.exit_to_tray" />
                        <span class="setting-label">{{ $t("settings.exit_to_tray") }}*</span>
                    </div>
                    <div class="setting-box">
                        <ElSwitch v-model="config.settings.log_to_file" />
                        <span class="setting-label">{{ $t("settings.log_to_file") }}*</span>
                    </div>
                    <div class="setting-box">
                        <ElSwitch v-model="isDark" />
                        <span class="setting-label">{{ $t("settings.enable_dark_mode") }}</span>
                    </div>
                </el-tab-pane>

                <!-- 备份设置 -->
                <el-tab-pane :label="$t('settings.backup_settings')" name="backup">
                    <el-divider content-position="left">
                        <el-icon>
                            <Document />
                        </el-icon>
                        <span class="tab-title">{{ $t('settings.backup_settings') }}</span>
                    </el-divider>

                    <div class="setting-box">
                        <ElSwitch v-model="config.settings.prompt_when_not_described" />
                        <span class="setting-label">{{ $t("settings.prompt_when_not_described") }}</span>
                    </div>
                    <div class="setting-box">
                        <ElSwitch v-model="config.settings.prompt_when_auto_backup" />
                        <span class="setting-label">{{ $t("settings.prompt_when_auto_backup") }}</span>
                    </div>
                    <div class="setting-box">
                        <ElSwitch v-model="config.settings.extra_backup_when_apply" />
                        <span class="setting-label">{{ $t("settings.extra_backup_when_apply") }}</span>
                    </div>
                    <div class="setting-box">
                        <ElSwitch v-model="config.settings.default_delete_before_apply" />
                        <span class="setting-label">{{ $t("settings.default_delete_before_apply") }}</span>
                    </div>
                    <div class="setting-box">
                        <ElSwitch v-model="config.settings.add_new_to_favorites" />
                        <span class="setting-label">{{ $t("settings.add_new_to_favorites") }}</span>
                    </div>
                </el-tab-pane>

                <!-- 界面设置 -->
                <el-tab-pane :label="$t('settings.ui_settings')" name="ui">
                    <el-divider content-position="left">
                        <el-icon>
                            <Moon />
                        </el-icon>
                        <span class="tab-title">{{ $t('settings.ui_settings') }}</span>
                    </el-divider>

                    <div class="setting-box">
                        <ElSelect v-model="config.settings.save_list_expand_behavior">
                            <ElOption :label="$t('settings.save_list_expand_behavior_default_open')" value="always_open" />
                            <ElOption :label="$t('settings.save_list_expand_behavior_default_closed')" value="always_closed" />
                            <ElOption :label="$t('settings.save_list_expand_behavior_remember_last')" value="remember_last" />
                        </ElSelect>
                        <span class="setting-label">{{ $t("settings.save_list_expand_behavior") }}</span>
                    </div>
                    <div class="setting-box">
                        <ElSwitch v-model="config.settings.default_expend_favorites_tree" />
                        <span class="setting-label">{{ $t("settings.default_expend_favorites_tree") }}</span>
                    </div>
                </el-tab-pane>
                
                <!-- 设备管理 -->
                <el-tab-pane :label="$t('settings.device_settings')" name="device">
                    <el-divider content-position="left">
                        <el-icon>
                            <Tools />
                        </el-icon>
                        <span class="tab-title">{{ $t('settings.device_settings') }}</span>
                    </el-divider>
                    
                    <!-- 当前设备信息 -->
                    <div class="setting-box">
                        <h3>{{ $t('settings.current_device') }}</h3>
                        <div class="device-info">
                            <el-form :model="currentDevice" label-position="top">
                                <el-form-item :label="$t('settings.device_name')">
                                    <el-input v-model="currentDevice.name" @change="updateDeviceInfo" />
                                </el-form-item>
                                <el-form-item :label="$t('settings.device_id')">
                                    <el-input v-model="currentDevice.id" disabled />
                                </el-form-item>
                            </el-form>
                        </div>
                    </div>
                    
                    <!-- 其他设备列表 -->
                    <div class="setting-box">
                        <h3>{{ $t('settings.other_devices') }}</h3>
                        <el-table :data="otherDevices" style="width: 100%">
                            <el-table-column prop="name" :label="$t('settings.device_name')" />
                            <el-table-column prop="id" :label="$t('settings.device_id')" width="220" />
                            <el-table-column :label="$t('settings.actions')" width="220">
                                <template #default="scope">
                                    <el-button @click="importFromDevice(scope.row.id)" type="primary" size="small">
                                        {{ $t('settings.import_paths') }}
                                    </el-button>
                                    <el-button @click="deleteDevice(scope.row.id)" type="danger" size="small" plain>
                                        {{ $t('settings.delete_device') }}
                                    </el-button>
                                </template>
                            </el-table-column>
                        </el-table>
                    </div>
                </el-tab-pane>

                <!-- 快捷键设置 -->
                <el-tab-pane :label="$t('settings.hotkey_settings')" name="hotkeys">
                    <el-divider content-position="left">
                        <el-icon>
                            <Unlock />
                        </el-icon>
                        <span class="tab-title">{{ $t('settings.hotkey_settings') }}</span>
                    </el-divider>

                    <div class="setting-box">
                        <div>
                            <strong v-if="config.quick_action!.quick_action_game">
                                {{ $t("setting.current_quick_action_game") }} :
                                {{ config.quick_action!.quick_action_game?.name }}
                            </strong>
                        </div>
                        <HotkeySelector v-model="config.quick_action!.hotkeys" />
                        <div class="quick-action-feedback">
                            <h4 class="sound-section-title">{{ $t('settings.quick_action_feedback_title') }}</h4>
                            <div class="setting-sub-box">
                                <ElSwitch v-model="config.quick_action!.notifications.enabled" />
                                <span class="setting-label">{{ $t('settings.quick_action_notifications_label') }}</span>
                            </div>
                            <div class="setting-sub-box">
                                <ElSwitch v-model="config.quick_action!.sound.enabled" />
                                <span class="setting-label">{{ $t('settings.quick_action_sound_label') }}</span>
                            </div>
                            <div v-if="config.quick_action!.sound.enabled" class="sound-setting-group">
                                <div class="sound-row">
                                    <span class="sound-label">{{ $t('settings.quick_action_sound_success') }}</span>
                                    <ElRadioGroup v-model="quickActionSuccessSoundMode" size="small">
                                        <ElRadioButton label="default">{{ $t('settings.quick_action_sound_mode_default') }}</ElRadioButton>
                                        <ElRadioButton label="custom">{{ $t('settings.quick_action_sound_mode_custom') }}</ElRadioButton>
                                    </ElRadioGroup>
                                    <ElButton
                                        size="small"
                                        type="primary"
                                        plain
                                        @click="previewQuickActionSound('success')"
                                    >
                                        {{ successPreviewButtonText }}
                                    </ElButton>
                                </div>
                                <div class="sound-path" v-if="quickActionSuccessSoundMode === 'custom'">
                                    <ElInput
                                        v-model="quickActionSuccessSoundPath"
                                        class="sound-input"
                                        :placeholder="$t('settings.quick_action_sound_placeholder')"
                                    />
                                    <ElButton size="small" @click="chooseQuickActionSound('success')">
                                        {{ $t('settings.quick_action_sound_choose') }}
                                    </ElButton>
                                </div>
                                <div class="sound-row">
                                    <span class="sound-label">{{ $t('settings.quick_action_sound_failure') }}</span>
                                    <ElRadioGroup v-model="quickActionFailureSoundMode" size="small">
                                        <ElRadioButton label="default">{{ $t('settings.quick_action_sound_mode_default') }}</ElRadioButton>
                                        <ElRadioButton label="custom">{{ $t('settings.quick_action_sound_mode_custom') }}</ElRadioButton>
                                    </ElRadioGroup>
                                    <ElButton
                                        size="small"
                                        type="primary"
                                        plain
                                        @click="previewQuickActionSound('failure')"
                                    >
                                        {{ failurePreviewButtonText }}
                                    </ElButton>
                                </div>
                                <div class="sound-path" v-if="quickActionFailureSoundMode === 'custom'">
                                    <ElInput
                                        v-model="quickActionFailureSoundPath"
                                        class="sound-input"
                                        :placeholder="$t('settings.quick_action_sound_placeholder')"
                                    />
                                    <ElButton size="small" @click="chooseQuickActionSound('failure')">
                                        {{ $t('settings.quick_action_sound_choose') }}
                                    </ElButton>
                                </div>
                            </div>
                        </div>
                        <div class="setting-action">
                            <el-button type="primary" @click="saveHotkeys" :disabled="!hotkeysChanged">
                                {{ $t("settings.save_hotkeys") }}
                            </el-button>
                            <el-tag v-if="hotkeysChanged" type="warning">{{ $t("settings.unsaved_changes") }}</el-tag>
                        </div>
                    </div>
                </el-tab-pane>

                <!-- 游戏排序 -->
                <el-tab-pane :label="$t('settings.game_order')" name="gameOrder">
                    <el-divider content-position="left">
                        <el-icon>
                            <Tools />
                        </el-icon>
                        <span class="tab-title">{{ $t('settings.game_order') }}</span>
                    </el-divider>

                    <div class="setting-box drag-game-box">
                        <!-- 移除handle属性，恢复原有的拖拽功能 -->
                        <draggable v-model="config.games" item-key="name" :force-fallback="true">
                            <template #item="{ element }">
                                <div class="game-order-box">
                                    {{ element.name }}
                                </div>
                            </template>
                        </draggable>
                        <div class="setting-action">
                            <el-button type="primary" @click="saveGameOrder" :disabled="!gameOrderChanged">
                                {{ $t("settings.save_game_order") }}
                            </el-button>
                            <el-tag v-if="gameOrderChanged" type="warning">{{ $t("settings.unsaved_changes") }}</el-tag>
                        </div>
                    </div>
                </el-tab-pane>
            </el-tabs>
        </el-card>
    </el-container>
</template>

<style scoped>
.el-button {
    margin-left: 0px important;
    margin-right: 10px;
    margin-top: 5px;
}

.el-card {
    overflow-y: auto;
    height: 100%;
}

.el-switch {
    margin-right: 20px;
}

.setting-box {
    margin-top: 15px;
    padding: 10px;
    border-radius: 4px;
    transition: background-color 0.3s;
}

.setting-box:hover {
    background-color: var(--el-fill-color-light);
}

.setting-label {
    margin-left: 10px;
    vertical-align: middle;
}

.setting-action {
    margin-top: 15px;
    display: flex;
    align-items: center;
    gap: 10px;
}

.quick-action-feedback {
    margin-top: 20px;
    display: flex;
    flex-direction: column;
    gap: 12px;
}

.sound-section-title {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
}

.setting-sub-box {
    display: flex;
    align-items: center;
    gap: 10px;
}

.sound-setting-group {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-left: 4px;
}

.sound-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px;
}

.sound-path {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px;
}

.sound-input {
    flex: 1;
    min-width: 200px;
}

.sound-label {
    font-weight: 500;
    min-width: 120px;
}

.tab-title {
    margin-left: 8px;
    font-weight: 600;
}

/** 以下是排序盒子样式 */
.game-order-box {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: medium;
    margin-top: 10px;
    padding: 10px;
    cursor: move;
    /* 更改游戏排序盒子的光标为move，提示可拖动 */
    transition: all 0.3s ease;
    border: 1px solid var(--el-border-color);
    border-radius: 4px;
}

.game-order-box:hover {
    box-shadow: var(--el-box-shadow-light);
    transform: translateY(-2px);
}

/** 以下是首页选择样式 */
.home-option-box {
    display: flex;
    align-items: center;
}

.home-box-icon {
    height: 1em;
    width: 1em;
    margin-right: 10px;
}

.drag-game-box {
    user-select: none;
}

.el-select {
    max-width: 200px;
}

.settings-tabs {
    margin-top: 20px;
}

.translate-website {
    cursor: pointer;
    color: var(--el-color-primary);
    text-decoration: none;
}
</style>