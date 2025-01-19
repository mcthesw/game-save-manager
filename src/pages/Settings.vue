<script lang="ts" setup>
// TODO:调整日志设置，比如删除日
// TODO:对设置进行分类
import { computed, ref, watch } from "vue";
import { $t } from "../i18n";
import { ElMessageBox, ElOption } from "element-plus";
import { useI18n } from "vue-i18n";
import draggable from 'vuedraggable'
import { DocumentAdd, HotWater, InfoFilled, MostlyCloudy, Setting, SwitchFilled } from "@element-plus/icons-vue";
import HotkeySelector from "../components/HotkeySelector.vue";
import { useDark } from '@vueuse/core'
import { commands } from "~/bindings";
import { error, info } from "@tauri-apps/plugin-log";

const isDark = useDark()
const { config, refreshConfig, saveConfig } = useConfig()
const { showSuccess, showError, showInfo } = useNotification()
const i18n = useI18n()
const locale_message = i18n.messages
const locale_names = i18n.availableLocales

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

watch(
    () => config.value.settings.locale,
    (new_locale, _old_locale) => {
        info(`locale changed to ${new_locale}`)
        if (new_locale)
            i18n.locale.value = new_locale
        showInfo({ message: $t("settings.locale_changed") });
    }
)

watch(
    () => config.value?.settings,
    async () => {
        try {
            await saveConfig();
        } catch (e) {
            error(`save config error: ${e}`)
            showError({ message: $t("error.set_config_failed") })
        }
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
            <div class="setting-box">
                <ElSelect v-model="config.settings.locale">
                    <ElOption v-for="locale in locale_names" :key="locale"
                        :label="(locale_message[locale] as any)['settings']['locale_name'] + ' - ' + locale"
                        :value="locale" />
                </ElSelect>
                🌍 Languages*
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
                🏠 {{ $t("settings.homepage") }}
            </div>
            <div class="setting-box">
                <ElSwitch v-model="config.settings.prompt_when_not_described" />
                <span>{{ $t("settings.prompt_when_not_described") }}</span>
            </div>
            <div class="setting-box">
                <ElSwitch v-model="config.settings.prompt_when_auto_backup" />
                <span>{{ $t("settings.prompt_when_auto_backup") }}</span>
            </div>
            <div class="setting-box">
                <ElSwitch v-model="config.settings.exit_to_tray" />
                <span>{{ $t("settings.exit_to_tray") }}*</span>
            </div>
            <div class="setting-box">
                <ElSwitch v-model="config.settings.extra_backup_when_apply" />
                <span>{{ $t("settings.extra_backup_when_apply") }}</span>
            </div>
            <div class="setting-box">
                <ElSwitch v-model="isDark" />
                <span>{{ $t("settings.enable_dark_mode") }}</span>
            </div>
            <-- TODO: 移除该功能 -->
                <!-- <div class="setting-box">
                    <ElSwitch v-model="config.settings.show_edit_button" />
                    <span>{{ $t("settings.enable_edit_manage") }}</span>
                </div> -->
                <div class="setting-box">
                    <ElSwitch v-model="config.settings.default_delete_before_apply" />
                    <span>{{ $t("settings.default_delete_before_apply") }}</span>
                </div>
                <div class="setting-box">
                    <ElSwitch v-model="config.settings.default_expend_favorites_tree" />
                    <span>{{ $t("settings.default_expend_favorites_tree") }}</span>
                </div>
                <div class="setting-box">
                    <ElSwitch v-model="config.settings.log_to_file" />
                    <span>{{ $t("settings.log_to_file") }}*</span>
                </div>
                <div class="setting-box">
                    <ElSwitch v-model="config.settings.add_new_to_favorites" />
                    <span>{{ $t("settings.add_new_to_favorites") }}</span>
                </div>
                <div class="setting-box drag-game-box">
                    <ElCollapse>
                        <ElCollapseItem :title="$t('settings.quick_action_hotkeys') + '*'">
                            <div>
                                <strong v-if="config.quick_action!.quick_action_game">
                                    {{ $t("setting.current_quick_action_game") }} :
                                    {{ config.quick_action!.quick_action_game?.name }}
                                </strong>
                            </div>
                            <HotkeySelector v-model="config.quick_action!.hotkeys" />
                        </ElCollapseItem>
                    </ElCollapse>
                </div>
                <div class="setting-box drag-game-box">
                    <ElCollapse>
                        <ElCollapseItem :title="$t('settings.adjust_game_order')">
                            <draggable v-model="config.games" item-key="name" :force-fallback="true">
                                <template #item="{ element }">
                                    <div class="game-order-box"> {{ element.name }} </div>
                                </template>
                            </draggable>
                        </ElCollapseItem>
                    </ElCollapse>
                </div>
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
        overflow-y: scroll;
    }

    .el-switch {
        margin-right: 20px;
    }

    .setting-box {
        margin-top: 10px;
    }

    /** 以下是排序盒子样式 */
    .game-order-box:hover {
        transition: box-shadow 0.3s ease;
        box-shadow: var(--el-box-shadow-light);
    }

    .game-order-box {
        font-size: medium;
        margin-top: 10px;
        padding: 5px;
        padding-left: 10px;
        cursor: pointer;
        transition: box-shadow 0.3s ease;
        border: 1px solid var(--el-border-color);
        border-radius: 4px;
    }

    /** 以上是排序盒子样式   */

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

    /** 以上是首页选择样式 */

    .drag-game-box {
        user-select: none;
    }

    .el-select {
        max-width: 200px;
    }
</style>