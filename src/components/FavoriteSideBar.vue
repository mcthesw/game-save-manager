<script lang="ts" setup>
import type Node from 'element-plus/es/components/tree/src/model/node'
import { ref, computed } from 'vue';
import { $t } from '../i18n';
import { v4 as uuidv4 } from 'uuid';
import type { AllowDropType } from 'element-plus/es/components/tree/src/tree.type';
import type { FavoriteTreeNode, Game } from '~/bindings';
import { Close, EditPen, FolderAdd, Plus } from '@element-plus/icons-vue';

const { config, saveConfig, refreshConfig } = useConfig();
const { showWarning, showSuccess, showError } = useNotification();
const enable_edit = ref(false);
const add_game_dialog_visible = ref(false);

// 接收从MainSideBar传递的搜索查询
const props = defineProps({
    searchQuery: {
        type: String,
        default: ''
    }
});

// 过滤收藏夹树
const filteredFavorites = computed(() => {
    if (!props.searchQuery || !config.value?.favorites) return config.value?.favorites;

    const query = props.searchQuery.toLowerCase();

    // 递归过滤函数
    const filterNodes = (nodes: FavoriteTreeNode[]): FavoriteTreeNode[] => {
        if (!nodes) return [];

        return nodes.filter((node: FavoriteTreeNode) => {
            // 检查当前节点是否匹配
            const nodeMatches = node.label.toLowerCase().includes(query);

            // 如果有子节点，递归过滤
            if (node.children && node.children.length > 0) {
                const filteredChildren: FavoriteTreeNode[] = filterNodes(node.children);
                node.children = filteredChildren;

                // 如果子节点有匹配或当前节点匹配，则保留
                return filteredChildren.length > 0 || nodeMatches;
            }

            // 叶子节点直接根据是否匹配决定
            return nodeMatches;
        });
    };

    // 创建一个深拷贝以避免修改原始数据
    const clonedFavorites = JSON.parse(JSON.stringify(config.value.favorites));
    return filterNodes(clonedFavorites);
});


function favorite_click_handler(node: FavoriteTreeNode) {
    // 四个参数，分别对应于节点点击的节点对象，TreeNode 的 node 属性, TreeNode和事件对象
    if (!node.is_leaf) {
        return;
    }
    if (!config.value?.games.find(x => x.name == node.label)) {
        showWarning({ message: $t('favorite.game_not_found') + ": " + node.label });
        return;
    }
    navigateTo("/Management/" + node.label)
}

function remove_node(node: Node, data: FavoriteTreeNode) {
    const parent = node.parent
    // 注意下面这行，正常来说parent.data.children是FavoriteTreeNode[]
    // 但当node节点在最外层时，需要取parent.data才对，这貌似是element-plus的问题
    const children: FavoriteTreeNode[] = parent.data.children || parent.data
    const index = children.findIndex((d) => d.node_id === data.node_id)
    children.splice(index, 1)
    config.value!.favorites = [...config.value!.favorites!]
    save_and_refresh()
    showSuccess({ message: $t('favorite.remove_success') });
}

function add_game_to_favorite(game: Game) {
    add_node(game.name, true)
    showSuccess({ message: $t('favorite.add_success') + ": " + game.name })
}

const save_and_refresh = useDebounceFn(async () => {
    await saveConfig();
}, 500);

function add_node(label: string, is_leaf: boolean, children: Array<FavoriteTreeNode> | null = null) {
    config.value?.favorites?.push({
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
    if (!name.value || name.value.length < 1 || config.value?.favorites?.find(x => x.label == name.value)) {
        showError({ message: $t('favorite.duplicated_empty_error') });
        return;
    }

    add_node(name.value, false, [])
}

function node_drag_end_handler(start: Node, end: Node, end_type: string, event: DragEvent) {
    console.log(start, end, end_type, event)
    if (end_type !== 'none') {
        // 如果成功，那么需要保存
        save_and_refresh()
    }
}

async function add_all_games() {
    try {
        await ElMessageBox.confirm(
            $t('favorite.confirm_add_all_games'),
            $t('home.hint'),
            {
                confirmButtonText: $t('settings.confirm'),
                cancelButtonText: $t('settings.cancel'),
                type: 'warning',
            }
        )
        
        // 首先创建一个集合来存储收藏夹中所有游戏的名称
        const existingGameNames = new Set<string>();
        
        // 递归遍历收藏夹树，查找所有叶子节点（游戏）
        function findExistingGames(nodes: FavoriteTreeNode[] | null) {
            if (!nodes) return;
            
            for (const node of nodes) {
                if (node.is_leaf) {
                    // 如果是叶子节点（游戏），添加到集合中
                    existingGameNames.add(node.label);
                } else if (node.children) {
                    // 如果是文件夹节点，递归查找其子节点
                    findExistingGames(node.children);
                }
            }
        }
        
        // 开始遍历收藏夹树
        findExistingGames(config.value?.favorites || []);
        
        // 遍历游戏列表，添加不在收藏夹中的游戏
        let addedCount = 0;
        for (const game of config.value!.games!) {
            if (!existingGameNames.has(game.name)) {
                add_game_to_favorite(game);
                addedCount++;
            }
        }
        
        if (addedCount > 0) {
            showSuccess({ message: $t('favorite.add_all_success').replace('{count}', addedCount.toString()) });
        } else {
            showWarning({ message: $t('favorite.no_new_games') });
        }
    } catch {
        // User cancelled
        return;
    }
}
</script>

<template>
    <div class="favorite-container">
        <div class="action-bar">
            <div class="action-buttons">
                <ElTooltip :content="$t('favorite.add_favorite_folder')" placement="bottom">
                    <ElButton :icon="FolderAdd" size="small" circle @click="add_folder" class="action-button" />
                </ElTooltip>
                <ElTooltip :content="$t('favorite.add_game')" placement="bottom">
                    <ElButton :icon="Plus" size="small" circle @click="() => add_game_dialog_visible = true"
                        class="action-button" />
                </ElTooltip>
                <ElTooltip :content="$t('favorite.enable_edit')" placement="bottom">
                    <ElButton :icon="EditPen" :type="enable_edit ? 'primary' : ''" size="small" circle
                        @click="() => { enable_edit = !enable_edit }" class="action-button" />
                </ElTooltip>
            </div>
        </div>
        <ElTree class="menu-item" :data="filteredFavorites" node-key="node_id" :draggable="enable_edit"
            :allow-drag="allow_drag" :allow-drop="allow_drop"
            :default-expand-all="config?.settings.default_expend_favorites_tree" @node-click="favorite_click_handler"
            @node-drag-end="node_drag_end_handler"
            :empty-text="props.searchQuery ? $t('misc.no_search_results') : $t('favorite.no_favorites')">
            <template #default="{ node, data }">
                <div v-if="data.is_leaf" class="custom-tree-node leaf-node">
                    <ElLink v-if="enable_edit" type="danger" :icon="Close" circle class="remove-btn"
                        @click.stop="remove_node(node, data)" />
                    <span class="node-label">{{ data.label }}</span>
                </div>
                <div v-else class="custom-tree-node folder-node">
                    <ElLink v-if="enable_edit" type="danger" :icon="Close" circle class="remove-btn"
                        @click.stop="remove_node(node, data)" />
                    <span class="folder-label">{{ data.label }}</span>
                </div>
            </template>
        </ElTree>
        <!-- 下方是用于选择新增游戏的Dialog -->
        <ElDialog v-model="add_game_dialog_visible" :title="$t('favorite.choose_game_add')">
            <ElTable :data="config?.games" :border="true" :height="500">
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
            <template #footer>
                <div style="text-align: right">
                    <ElButton type="primary" @click="add_all_games">{{ $t('favorite.add_all_games') }}</ElButton>
                </div>
            </template>
        </ElDialog>
        <!-- 上方是用于选择新增游戏的Dialog -->
    </div>
</template>

<style scoped>
.favorite-container {
    height: 100%;
    display: flex;
    flex-direction: column;
}

.action-bar {
    display: flex;
    justify-content: center;
    margin-bottom: 8px;
    padding: 6px 8px;
    background-color: var(--el-bg-color-overlay);
    border-radius: 8px;
    margin: 0 8px 8px 8px;
}

.action-buttons {
    display: flex;
    gap: 12px;
}

.action-button {
    transition: transform 0.2s ease, box-shadow 0.2s ease;
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.05);
}

.action-button:hover {
    transform: translateY(-2px);
    box-shadow: 0 4px 8px rgba(0, 0, 0, 0.1);
}

/* 以下部分用于支持多行树组件 - 允许完整显示长文本 */
.custom-tree-node {
    flex: 1;
    white-space: normal;
    overflow: visible;
    word-break: break-word;
    line-height: 1.4;
    display: flex;
    align-items: center;
    gap: 4px;
}

.leaf-node {
    padding: 2px 0;
}

.folder-node {
    padding: 2px 0;
}

.node-label {
    font-size: 0.95rem;
    color: var(--el-text-color-primary);
}

.folder-label {
    font-weight: 600;
    font-size: 0.95rem;
    color: var(--el-text-color-primary);
}

.remove-btn {
    opacity: 0.7;
    transition: opacity 0.2s ease;
}

.remove-btn:hover {
    opacity: 1;
}

:deep(.el-tree-node__content) {
    text-align: left;
    align-items: center;
    margin: 2px;
    height: auto;
    padding: 4px 0;
    border-radius: 4px;
    transition: background-color 0.2s ease;
}

:deep(.el-tree) {
    padding: 0 8px;
    background-color: transparent;
}

:deep(.el-tree-node__content:hover) {
    background-color: var(--el-fill-color-light);
}

:deep(.el-tree-node.is-current > .el-tree-node__content) {
    background-color: var(--el-color-primary-light-9) !important;
    color: var(--el-color-primary);
}

/* 文件夹节点样式 */
:deep(.el-tree-node:not(.is-leaf) > .el-tree-node__content) {
    background-color: var(--el-bg-color-overlay);
    margin-bottom: 4px;
    border-radius: 6px;
}

:deep(.el-tree-node__label) {
    width: 100%;
}

/* 优化空状态显示 */
:deep(.el-tree__empty-block) {
    min-height: 60px;
}

:deep(.el-tree__empty-text) {
    color: var(--el-text-color-secondary);
}

/* 以上部分用于支持双行树组件 */
</style>
