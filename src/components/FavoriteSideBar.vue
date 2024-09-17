<script lang="ts" setup>
import { useConfig } from '../stores/ConfigFile';
import type Node from 'element-plus/es/components/tree/src/model/node'
import { FavoriteTreeNode, Game } from '../schemas/saveTypes';
import { useRouter } from 'vue-router';
import { ElButton, ElLink, ElMessageBox, ElTooltip } from 'element-plus';
import { ref } from 'vue';
import { $t } from '../i18n';
import { show_error, show_success, show_warning } from '../utils/notifications';
import { v4 as uuidv4 } from 'uuid';
import { invoke } from '@tauri-apps/api';
import { AllowDropType } from 'element-plus/es/components/tree/src/tree.type';
import { Close, EditPen, FolderAdd, Plus } from '@element-plus/icons-vue';

const config = useConfig();
const router = useRouter();
const enable_edit = ref(false);
const add_game_dialog_visible = ref(false);


function favorite_click_handler(node: FavoriteTreeNode) {
    // 四个参数，分别对应于节点点击的节点对象，TreeNode 的 node 属性, TreeNode和事件对象
    if (!node.is_leaf) {
        return;
    }
    if (!config.games.find(x => x.name == node.label)) {
        show_warning($t('favorite.game_not_found') + ": " + node.label);
        return;
    }
    router.push("/management/" + node.label)
}

function remove_node(node: Node, data: FavoriteTreeNode) {
    const parent = node.parent
    // 注意下面这行，正常来说parent.data.children是FavoriteTreeNode[]
    // 但当node节点在最外层时，需要取parent.data才对，这貌似是element-plus的问题
    const children: FavoriteTreeNode[] = parent.data.children || parent.data
    const index = children.findIndex((d) => d.node_id === data.node_id)
    children.splice(index, 1)
    config.favorites = [...config.favorites]
    save_and_refresh()
    show_success($t('favorite.remove_success'));
}

function add_game_to_favorite(game: Game) {
    add_node(game.name, true)
    show_success($t('favorite.add_success') + ": " + game.name)
}

function save_and_refresh() {
    invoke("set_config", { config: config.$state }).catch(
        (e) => {
            console.log(e)
            show_error($t("error.set_config_failed"))
        }
    ).finally(
        () => {
            config.refresh()
        }
    )
}

function add_node(label: string, is_leaf: boolean, children?: Array<FavoriteTreeNode>) {
    config.favorites?.push({
        label: label,
        is_leaf: is_leaf,
        children: children,
        node_id: uuidv4().toString()
    })
    save_and_refresh()
}

function allow_drag(node: FavoriteTreeNode) {
    return true;
}

function allow_drop(draggingNode: Node, dropNode: Node, type: AllowDropType) {
    if (dropNode.data.is_leaf) {
        // 防止拖拽游戏到游戏内部
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
    name.value = name.value.trim()
    // 检查是否已经存在
    if (!name.value || name.value.length < 1 || config.favorites?.find(x => x.label == name.value)) {
        show_error($t('favorite.duplicated_empty_error'));
        return;
    }

    add_node(name.value, false, [])
}
</script>

<template>
    <div class="favorite-container">
        <div class="action-bar">
            <ElTooltip :content="$t('favorite.add_favorite_folder')">
                <ElButton :icon="FolderAdd" size="small" circle @click="add_folder" />
            </ElTooltip>
            <ElTooltip :content="$t('favorite.add_game')">
                <ElButton :icon="Plus" size="small" circle @click="() => add_game_dialog_visible = true" />
            </ElTooltip>
            <ElTooltip :content="$t('favorite.enable_edit')">
                <ElButton :icon="EditPen" :type="enable_edit ? 'primary' : ''" size="small" circle
                    @click="() => { enable_edit = !enable_edit }" />
            </ElTooltip>
        </div>
        <ElTree class="menu-item" :data="config.favorites" node-key="node_id" :draggable="enable_edit"
            :allow-drag="allow_drag" :allow-drop="allow_drop"
            :default-expand-all="config.settings.default_expend_favorites_tree" @node-click="favorite_click_handler">
            <template #default="{ node, data }">
                <span v-if="data.is_leaf" class="custom-tree-node">
                    <ElLink v-if="enable_edit" type="danger" :icon="Close" circle
                        @click.stop="remove_node(node, data)" />
                    {{ data.label }}
                </span>
                <strong v-else class="custom-tree-node">
                    <ElLink v-if="enable_edit" type="danger" :icon="Close" circle
                        @click.stop="remove_node(node, data)" />
                    {{ data.label }}
                </strong>
            </template>
        </ElTree>
        <!-- 下方是用于选择新增游戏的Dialog -->
        <ElDialog v-model="add_game_dialog_visible" :title="$t('favorite.choose_game_add')">
            <ElTable :data="config.games" :border="true" :height="500">
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
.action-bar {
    display: flex;
    justify-content: space-evenly;
    margin-bottom: 10px;
}

/* 以下部分用于支持双行树组件 */
.custom-tree-node {
    flex: 1;
    white-space: normal;
}

:deep(.el-tree-node__content) {
    text-align: left;
    align-items: start;
    margin: 4px;
    height: 100%;
}

/* 以上部分用于支持双行树组件 */
</style>
