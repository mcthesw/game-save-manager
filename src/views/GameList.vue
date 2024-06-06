<script setup lang="ts">
import { ElButton, ElCard, ElContainer, ElDialog, ElLink, ElMessageBox, ElScrollbar, ElSwitch, ElTooltip, ElTree } from 'element-plus';
import { useConfig } from '../stores/ConfigFile';
import { AllowDropType } from 'element-plus/es/components/tree/src/tree.type';
import type Node from 'element-plus/es/components/tree/src/model/node'
import { ref } from 'vue';
import { FavoriteTreeNode, Game } from '../schemas/saveTypes';
import { $t } from '../i18n';
import { show_error, show_info, show_success } from '../utils/notifications';
import { v4 as uuidv4 } from 'uuid'
import { invoke } from '@tauri-apps/api';
import { useRouter } from 'vue-router';

const config = useConfig();
const router = useRouter()
const enable_drag = ref(false);
const dialog_visible = ref(false);

function add_node(label: string, is_leaf: boolean, children?: Array<FavoriteTreeNode>) {
    config.favorites?.push({
        label: label,
        is_leaf: is_leaf,
        children: children,
        node_id: uuidv4().toString()
    })
}

function allow_drag(node: FavoriteTreeNode) {
    return true;
}

function allow_drop(draggingNode: Node, dropNode: Node, type: AllowDropType) {
    if (dropNode.data.is_leaf) {
        // 防止拖拽游戏到游戏
        return type !== 'inner';
    }
    return true;
}

async function add_folder() {
    // 弹出对话框
    let name: any;
    try {
        name = await ElMessageBox.prompt(
            $t("favorite.new_folder_name"),
            $t('home.hint'),
            {
                confirmButtonText: $t('settings.confirm'),
                cancelButtonText: $t('settings.cancel'),
            }
        )
    } catch {
        return
    }
    console.log(name.value);
    // 检查是否已经存在
    if (!name.value || name.value.length < 1 || config.favorites?.find(x => x.label == name.value)) {
        show_error($t('favorite.duplicated_empty_error'));
        return;
    }

    add_node(name.value, false, [])
}

function load_config() {
    config.refresh()
}

function save_favorite() {
    invoke("set_config", { config: config.$state }).then((x) => {
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
    load_config()
    show_success($t('settings.abort_change'))
}

function remove_node(node: Node, data: FavoriteTreeNode) {
    const parent = node.parent
    const children: FavoriteTreeNode[] = parent.data.children || parent.data
    const index = children.findIndex((d) => d.node_id === data.node_id)
    children.splice(index, 1)
    config.favorites = [...config.favorites]
    show_success($t('favorite.remove_success'));
}

function add_game_to_favorite(game: Game) {
    add_node(game.name, true)
    show_success($t('favorite.add_success') + ": " + game.name)
}

function manage_game(tree_node: FavoriteTreeNode) {
    router.push("/management/" + tree_node.label)
}
</script>

<template>
    <div>
        <ElCard>
            <div class="card-title">{{ $t("favorite.favorite_manage") }}</div>
            <div class="button-bar">
                <ElButton type="primary" @click="add_folder" round>{{ $t("favorite.add_favorite_folder") }}
                </ElButton>
                <ElButton type="primary" @click="() => dialog_visible = true" round>{{ $t("favorite.add_game") }}
                </ElButton>
                <ElButton type="primary" @click="save_favorite" round>{{ $t("favorite.save_favorite") }}</ElButton>
                <ElButton type="primary" @click="abort_change" round> {{ $t("favorite.abort_change") }} </ElButton>
            </div>
            <div class="button-bar">
                <ElSwitch type="primary" v-model="enable_drag" :active-text="$t('favorite.enable_drag')" round />
            </div>
            <ElTree :data="config.favorites" :draggable="enable_drag" node-key="node_id" :allow-drag="allow_drag"
                :allow-drop="allow_drop" class="favorite-tree">
                <template #default="{ node, data }">
                    <span class="custom-tree-node">
                        <div class="left-label" v-if="data.is_leaf">{{ data.label }}</div>
                        <strong v-else class="custom-tree-node">
                            {{ data.label }}
                        </strong>
                        <div class="right-btn">
                            <ElLink type="primary" :disabled="!data.is_leaf" v-show="data.is_leaf"
                                @click="manage_game(data)">{{ $t("favorite.jump_to_game") }}
                            </ElLink>
                            <ElLink type="primary" @click="remove_node(node, data)">
                                {{ $t("favorite.remove") }}
                            </ElLink>
                        </div>
                    </span>
                </template>
            </ElTree>
        </ElCard>
        <!-- 下方是用于选择新增游戏的Dialog -->
        <ElDialog v-model="dialog_visible" :title="$t('favorite.choose_game_add')">
            <ElTable :data="config.games" :border="true">
                <ElTableColumn prop="name" :label="$t('settings.name')" width="180" />
                <ElTableColumn prop="game_path" :label="$t('settings.game_path')" />
                <ElTableColumn fixed="right" :label="$t('settings.operation')" width="120">
                    <template #default="scope">
                        <ElButton link type="primary" size="small" @click="add_game_to_favorite(scope.row)">
                            {{ $t("favorite.add_to_favorite") }}
                        </ElButton>
                    </template>
                </ElTableColumn>
            </ElTable>
        </ElDialog>
        <!-- 上方是用于选择新增游戏的Dialog -->
    </div>
</template>

<style scoped>
.el-card {
    margin-bottom: 15px;
}

.card-title {
    font-weight: bold;
    font-size: 1.5rem;
}

.button-bar {
    margin-top: 10px;
    margin-bottom: 10px;
}

/* 以下部分用于支持双行树组件 */
.custom-tree-node {
    flex: 1;
    white-space: normal;
}

.custom-tree-node .el-link {
    margin-right: 10px;
}

:deep(.el-tree-node__content) {
    text-align: left;
    align-items: start;
    margin: 4px;
    height: 100%;
}

.left-label {
    float: left;
    /* 按钮宽度为100px */
    width: calc(100% - 150px);
    margin-right: 10px;
}

.right-btn {
    float: right;
    width: 140px;
}

/* 以上部分用于支持双行树组件 */
</style>