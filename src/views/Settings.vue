<script lang="ts" setup>
// TODO:调整日志设置，比如删除日
import { computed, ref, watch } from "vue";
import { useConfig } from "../stores/ConfigFile";
import { invoke } from "@tauri-apps/api/tauri";
import { show_error, show_info, show_success } from "../utils/notifications";
import { Game } from "../schemas/saveTypes";
import { useDark, useToggle } from '@vueuse/core'
import { $t } from "../i18n";
import { ElMessageBox, ElOption } from "element-plus";
import { useI18n } from "vue-i18n";
import draggable from 'vuedraggable'
import { DocumentAdd, Grid, HotWater, InfoFilled, MostlyCloudy, Setting, SwitchFilled } from "@element-plus/icons-vue";


const isDark = useDark()
const config = useConfig()
const loading = ref(false)
const i18n = useI18n()
const locale_message = i18n.messages
const locale_names = i18n.availableLocales

function load_config() {
    config.refresh()
}
function submit_settings() {
    loading.value = true;
    invoke("set_config", { config: config.$state }).then((x) => {
        loading.value = false;
        show_success($t("settings.submit_success"));
        load_config()
    }).catch(
        (e) => {
            console.log(e)
            show_error($t("error.set_config_failed"))
        }
    )
}
function abort_change() {
    show_success($t("settings.reset_success"));
    load_config();
}
function reset_settings() {
    invoke("reset_settings").then((x) => {
        show_success($t("settings.reset_success"));
        load_config();
    }).catch(
        (e) => {
            console.log(e)
            show_error($t("error.reset_settings_failed"))
        }
    )
}

function backup_all() {
    ElMessageBox.prompt(
        $t('settings.backup_all_hint'),
        $t('home.hint'),
        {
            confirmButtonText: $t('settings.confirm'),
            cancelButtonText: $t('settings.cancel'),
            inputPattern: /yes/,
            inputErrorMessage: $t('settings.invalid_input_error'),
        }
    )
        .then(() => {
            invoke("backup_all").then((x) => {
                show_success($t("settings.success"));
            }).catch(
                (e) => {
                    console.log(e)
                    show_error($t("settings.failed"))
                }
            )
        })
        .catch(() => {
            show_info($t('settings.operation_canceled'));
        });
}

function apply_all() {
    ElMessageBox.prompt(
        $t('settings.apply_all_hint'),
        $t('home.hint'),
        {
            confirmButtonText: $t('settings.confirm'),
            cancelButtonText: $t('settings.cancel'),
            inputPattern: /yes/,
            inputErrorMessage: $t('settings.invalid_input_error'),
        }
    )
        .then(() => {
            invoke("apply_all").then((x) => {
                show_success($t("settings.success"));
            }).catch(
                (e) => {
                    console.log(e)
                    show_error($t("settings.failed"))
                }
            )
        })
        .catch(() => {
            show_info($t('settings.operation_canceled'));
        });
}

function open_log_folder() {
    invoke("open_url", { url: "log" })
        .catch(
            (e) => {
                console.log(e)
                show_error($t('error.open_log_folder_failed'))
            }
        )
}

watch(
    () => config.settings.locale,
    (new_locale, _old_locale) => {
        console.log(new_locale)
        i18n.locale.value = new_locale
        show_info($t("settings.locale_changed"));
    }
)

const router_list = computed(() => {
    // TODO:抽离到新文件中，同时`MainSideBar.vue`也要抽离
    var link_list = [
        { text: $t("sidebar.homepage"), link: "/home", icon: HotWater },
        { text: $t("sidebar.add_game"), link: "/add-game", icon: DocumentAdd },
        { text: $t('sidebar.favorite_manage'), link: "/favorite", icon: Grid },
        { text: $t("sidebar.sync_settings"), link: "/sync-settings", icon: MostlyCloudy },
        { text: $t("sidebar.settings"), link: "/settings", icon: Setting },
        { text: $t("sidebar.about"), link: "/about", icon: InfoFilled },
    ]
    config.games.forEach((game) => {
        link_list.push({ text: game.name, link: `/management/${game.name}`, icon: SwitchFilled })
    })
    return link_list
})
</script>

<template>
    <el-container class="setting" direction="vertical">
        <el-card>
            <h1>{{ $t("settings.customizable_settings") }}</h1>
            <p>{{ $t("settings.setting_tips") }}</p>
            <div class="button-bar">
                <el-button @click="submit_settings()">{{ $t("settings.submit_settings") }}</el-button>
                <el-button @click="abort_change()">{{ $t("settings.abort_change") }}</el-button>
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
                <ElSelect :loading="loading" v-model="config.settings.locale">
                    <ElOption v-for="locale in locale_names" :key="locale"
                        :label="(locale_message[locale] as any)['settings']['locale_name'] + ' - ' + locale"
                        :value="locale" />
                </ElSelect>
                🌍 Languages*
            </div>
            <div class="setting-box">
                <ElSelect :loading="loading" v-model="config.settings.home_page">
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
                <ElSwitch v-model="config.settings.prompt_when_not_described" :loading="loading" />
                <span>{{ $t("settings.prompt_when_not_described") }}</span>
            </div>
            <div class="setting-box">
                <ElSwitch v-model="config.settings.prompt_when_auto_backup" :loading="loading" />
                <span>{{ $t("settings.prompt_when_auto_backup") }}</span>
            </div>
            <div class="setting-box">
                <ElSwitch v-model="config.settings.exit_to_tray" :loading="loading" />
                <span>{{ $t("settings.exit_to_tray") }}*</span>
            </div>
            <div class="setting-box">
                <ElSwitch v-model="config.settings.extra_backup_when_apply" :loading="loading" />
                <span>{{ $t("settings.extra_backup_when_apply") }}</span>
            </div>
            <div class="setting-box">
                <ElSwitch v-model="isDark" :loading="loading" />
                <span>{{ $t("settings.enable_dark_mode") }}</span>
            </div>
            <div class="setting-box">
                <ElSwitch v-model="config.settings.show_edit_button" :loading="loading" />
                <span>{{ $t("settings.enable_edit_manage") }}</span>
            </div>
            <div class="setting-box">
                <ElSwitch v-model="config.settings.default_delete_before_apply" :loading="loading" />
                <span>{{ $t("settings.default_delete_before_apply") }}</span>
            </div>
            <div class="setting-box">
                <ElSwitch v-model="config.settings.default_expend_favorites_tree" :loading="loading" />
                <span>{{ $t("settings.default_expend_favorites_tree") }}</span>
            </div>
            <div class="setting-box">
                <ElSwitch v-model="config.settings.log_to_file" :loading="loading" />
                <span>{{ $t("settings.log_to_file") }}*</span>
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
    margin-left: 0px !important;
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
