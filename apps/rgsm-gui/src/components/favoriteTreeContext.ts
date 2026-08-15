import type { InjectionKey, Ref } from 'vue';
import type { FavoriteTreeNode } from '../api/commands';

export type DropPos = 'before' | 'after' | 'inner';

export interface FavoriteTreeCtx {
  editMode: Ref<boolean>;
  /** 搜索中强制展开全部，保证匹配结果可见 */
  searching: Ref<boolean>;
  expandedIds: Ref<Set<string>>;
  toggleExpand: (id: string) => void;
  dropTarget: Ref<{ id: string; pos: DropPos } | null>;
  onDragStart: (id: string, e: DragEvent) => void;
  onDragOver: (id: string, isLeaf: boolean, e: DragEvent) => void;
  onDragLeave: (id: string) => void;
  onDrop: (id: string) => void;
  onDragEnd: () => void;
  removeNode: (id: string) => void;
  clickLeaf: (node: FavoriteTreeNode) => void;
}

export const FAVORITE_TREE_CTX: InjectionKey<FavoriteTreeCtx> = Symbol('favorite-tree-ctx');

// ——— 纯树操作：就地修改传入的根数组，调用方负责触发响应式并持久化 ———

export function findNode(nodes: FavoriteTreeNode[], id: string): FavoriteTreeNode | null {
  for (const node of nodes) {
    if (node.node_id === id) return node;
    if (!node.is_leaf && node.children) {
      const hit = findNode(node.children, id);
      if (hit) return hit;
    }
  }
  return null;
}

/** 找到包含指定节点的兄弟列表（顶层即根数组本身） */
export function findParentList(nodes: FavoriteTreeNode[], id: string): FavoriteTreeNode[] | null {
  if (nodes.some((node) => node.node_id === id)) return nodes;
  for (const node of nodes) {
    if (!node.is_leaf && node.children) {
      const hit = findParentList(node.children, id);
      if (hit) return hit;
    }
  }
  return null;
}

/** id 是否在以 node 为根的子树内（含 node 自身） */
export function isInsideSubtree(node: FavoriteTreeNode, id: string): boolean {
  if (node.node_id === id) return true;
  return (node.children ?? []).some((child) => isInsideSubtree(child, id));
}

export function removeById(nodes: FavoriteTreeNode[], id: string): boolean {
  const list = findParentList(nodes, id);
  if (!list) return false;
  const index = list.findIndex((node) => node.node_id === id);
  list.splice(index, 1);
  return true;
}

/**
 * 移动节点。before/after 相对目标兄弟排序，inner 放进文件夹末尾。
 * 拒绝：拖到自己、拖进自己的子树、inner 到游戏叶子。
 */
export function moveNode(
  root: FavoriteTreeNode[],
  dragId: string,
  targetId: string,
  pos: DropPos
): boolean {
  if (dragId === targetId) return false;
  const dragList = findParentList(root, dragId);
  const targetList = findParentList(root, targetId);
  if (!dragList || !targetList) return false;
  const dragIndex = dragList.findIndex((node) => node.node_id === dragId);
  const dragNode = dragList[dragIndex];
  if (isInsideSubtree(dragNode, targetId)) return false;
  const targetNode = targetList.find((node) => node.node_id === targetId)!;
  if (pos === 'inner' && targetNode.is_leaf) return false;

  dragList.splice(dragIndex, 1);
  if (pos === 'inner') {
    targetNode.children = targetNode.children ?? [];
    targetNode.children.push(dragNode);
    return true;
  }
  // 同列表先删后插会让目标索引位移，因此删除后重算
  const targetIndex = targetList.findIndex((node) => node.node_id === targetId);
  targetList.splice(pos === 'before' ? targetIndex : targetIndex + 1, 0, dragNode);
  return true;
}

/** 搜索过滤：保留自身或后代命中的文件夹。返回新数组，不改原树。 */
export function filterTree(nodes: FavoriteTreeNode[], query: string): FavoriteTreeNode[] {
  const q = query.trim().toLowerCase();
  if (!q) return nodes;
  const out: FavoriteTreeNode[] = [];
  for (const node of nodes) {
    if (node.is_leaf) {
      if (node.label.toLowerCase().includes(q)) out.push(node);
      continue;
    }
    const children = filterTree(node.children ?? [], q);
    if (node.label.toLowerCase().includes(q) || children.length > 0) {
      out.push({ ...node, children });
    }
  }
  return out;
}

/** 收集所有叶子（游戏）名称 */
export function collectLeafNames(
  nodes: FavoriteTreeNode[] | null | undefined,
  out: Set<string> = new Set()
): Set<string> {
  for (const node of nodes ?? []) {
    if (node.is_leaf) out.add(node.label);
    else collectLeafNames(node.children, out);
  }
  return out;
}

/** 统计子树内叶子（游戏）数量 */
export function countLeaves(node: FavoriteTreeNode): number {
  if (node.is_leaf) return 1;
  return (node.children ?? []).reduce((sum, child) => sum + countLeaves(child), 0);
}

/** 收集所有文件夹 id（用于默认全展开） */
export function collectFolderIds(
  nodes: FavoriteTreeNode[] | null | undefined,
  out: Set<string> = new Set()
): Set<string> {
  for (const node of nodes ?? []) {
    if (!node.is_leaf) {
      out.add(node.node_id);
      collectFolderIds(node.children, out);
    }
  }
  return out;
}
