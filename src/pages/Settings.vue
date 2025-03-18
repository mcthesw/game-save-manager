<script lang="ts" setup>
// TODO:调整日志设置，比如删除日
import { computed, ref, watch } from "vue";
import { $t, i18n } from "../i18n";
import { ElMessageBox, ElOption } from "element-plus";
import { useI18n } from "vue-i18n";
import draggable from 'vuedraggable'
import { DocumentAdd, HotWater, InfoFilled, MostlyCloudy, Setting, SwitchFilled, Document, Unlock, Moon, Tools } from "@element-plus/icons-vue";
import HotkeySelector from "../components/HotkeySelector.vue";
import { useDark } from '@vueuse/core'
import { commands } from "~/bindings";
import { error, info } from "@tauri-apps/plugin-log";

const isDark = useDark()
const { config, refreshConfig, saveConfig } = useConfig()
const { showSuccess, showError, showInfo } = useNotification()
const locale_message = i18n.global.messages
const locale_names = i18n.global.availableLocales
const activeTab = ref('general')
const hotkeysChanged = ref(false)
const gameOrderChanged = ref(false)

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
            await commands.backupAll();
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
        await commands.applyAll();
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
        // TODO: 后面搞个专门的接口
        commands.openUrl("log")
    } catch (e) {
        error(`open log folder error: ${e}`)
        showError({ message: $t('error.open_log_folder_failed') })
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
        await commands.openUrl("https://hosted.weblate.org/projects/game-save-manager")
    } catch (e) {
        error(`open translate website error: ${e}`)
    }
}

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

watch(
    () => config.value.settings.locale,
    (new_locale, _old_locale) => {
        info(`locale changed to ${new_locale}`)
        if (new_locale)
            i18n.global.locale.value = new_locale
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
        link_list.push({ text: game.name, link: `/management/${game.name}`, icon: SwitchFilled })
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
                                :label="(locale_message[locale] as any)['settings']['locale_name'] + ' - ' + locale"
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
                        <ElSwitch v-model="config.settings.default_expend_favorites_tree" />
                        <span class="setting-label">{{ $t("settings.default_expend_favorites_tree") }}</span>
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