<script setup lang="ts">
import { ref, watch } from 'vue';
import { VueFlow, useVueFlow } from '@vue-flow/core';
import { Background, BackgroundVariant } from '@vue-flow/background';
import type { Node, Edge } from '@vue-flow/core';
import SnapshotNode from './SnapshotNode.vue';
import type { Snapshot } from '../bindings';
import { $t } from '../i18n';
import { Aim, FullScreen } from '@element-plus/icons-vue';

// Import Vue Flow styles
import '@vue-flow/core/dist/style.css';
import '@vue-flow/core/dist/theme-default.css';

interface Props {
  snapshots: Snapshot[];
  head: string | null;
}

const props = defineProps<Props>();

const emit = defineEmits<{
  apply: [date: string];
  delete: [date: string];
  changeDescription: [date: string];
  setHead: [date: string];
  detach: [date: string];
  createBranch: [date: string];
}>();

const vueFlowRef = ref<InstanceType<typeof VueFlow> | null>(null);
const { fitView, setCenter } = useVueFlow();

const NODE_WIDTH = 160;
const NODE_HEIGHT = 100;
const HORIZONTAL_GAP = 40;
const VERTICAL_GAP = 60;

interface TreeNode {
  snapshot: Snapshot;
  children: TreeNode[];
  x: number;
  y: number;
  depth: number;
  mod: number;
  prelim: number;
  shift: number;
  change: number;
  thread: TreeNode | null;
  ancestor: TreeNode;
  number: number;
  parent: TreeNode | null;
}

// Build tree structure using simple layout algorithm
function buildTreeLayout(snapshots: Snapshot[]): { nodes: Node[]; edges: Edge[] } {
  if (snapshots.length === 0) {
    return { nodes: [], edges: [] };
  }

  // Build parent-child map
  const snapshotMap = new Map<string, Snapshot>();
  const childrenMap = new Map<string, Snapshot[]>();

  for (const snapshot of snapshots) {
    snapshotMap.set(snapshot.date, snapshot);
  }

  // Group children by parent
  for (const snapshot of snapshots) {
    const parentKey = snapshot.parent ?? '__root__';
    if (!childrenMap.has(parentKey)) {
      childrenMap.set(parentKey, []);
    }
    childrenMap.get(parentKey)!.push(snapshot);
  }

  // Sort children by date (oldest first, so they appear at bottom)
  for (const children of childrenMap.values()) {
    children.sort((a, b) => a.date.localeCompare(b.date));
  }

  // Find root nodes (no parent or parent not in list)
  const snapshotDates = new Set(snapshots.map((s) => s.date));
  const rootSnapshots = snapshots.filter(
    (s) => s.parent === null || s.parent === undefined || !snapshotDates.has(s.parent)
  );
  rootSnapshots.sort((a, b) => a.date.localeCompare(b.date));

  // Build tree nodes recursively
  function buildTreeNode(snapshot: Snapshot, parent: TreeNode | null, depth: number): TreeNode {
    const node: TreeNode = {
      snapshot,
      children: [],
      x: 0,
      y: depth,
      depth,
      mod: 0,
      prelim: 0,
      shift: 0,
      change: 0,
      thread: null,
      ancestor: null as unknown as TreeNode,
      number: 0,
      parent,
    };
    node.ancestor = node;

    const children = childrenMap.get(snapshot.date) || [];
    node.children = children.map((child, index) => {
      const childNode = buildTreeNode(child, node, depth + 1);
      childNode.number = index;
      return childNode;
    });

    return node;
  }

  // Create trees from root nodes
  const trees: TreeNode[] = rootSnapshots.map((s, i) => {
    const node = buildTreeNode(s, null, 0);
    node.number = i;
    return node;
  });

  // Simple tree layout: assign X positions using post-order traversal
  function layoutTree(node: TreeNode, minX: number): number {
    if (node.children.length === 0) {
      node.x = minX;
      return minX + 1;
    }

    let nextX = minX;
    for (const child of node.children) {
      nextX = layoutTree(child, nextX);
    }

    // Position parent in center of children
    const firstChild = node.children[0]!;
    const lastChild = node.children[node.children.length - 1]!;
    node.x = (firstChild.x + lastChild.x) / 2;

    return nextX;
  }

  // Find max depth for Y positioning
  function getMaxDepth(node: TreeNode): number {
    if (node.children.length === 0) return node.depth;
    return Math.max(...node.children.map(getMaxDepth));
  }

  // Layout all trees side by side
  let currentMinX = 0;
  let globalMaxDepth = 0;

  for (const tree of trees) {
    currentMinX = layoutTree(tree, currentMinX) + 1;
    globalMaxDepth = Math.max(globalMaxDepth, getMaxDepth(tree));
  }

  // Convert to Vue Flow nodes and edges
  const nodes: Node[] = [];
  const edges: Edge[] = [];

  function collectNodesAndEdges(node: TreeNode) {
    const posX = node.x * (NODE_WIDTH + HORIZONTAL_GAP);
    const posY = (globalMaxDepth - node.depth) * (NODE_HEIGHT + VERTICAL_GAP);

    nodes.push({
      id: node.snapshot.date,
      type: 'snapshot',
      position: { x: posX, y: posY },
      data: {
        snapshot: node.snapshot,
        isHead: node.snapshot.date === props.head,
        isRoot: node.parent === null,
      },
    });

    if (node.parent) {
      edges.push({
        id: `${node.snapshot.date}-${node.parent.snapshot.date}`,
        source: node.snapshot.date,
        target: node.parent.snapshot.date,
        type: 'smoothstep',
        style: {
          stroke: 'var(--el-color-primary)',
          strokeWidth: 2,
        },
        animated: node.snapshot.date === props.head,
      });
    }

    for (const child of node.children) {
      collectNodesAndEdges(child);
    }
  }

  for (const tree of trees) {
    collectNodesAndEdges(tree);
  }

  return { nodes, edges };
}

const flowNodes = ref<Node[]>([]);
const flowEdges = ref<Edge[]>([]);
const isReady = ref(false);

// Rebuild tree when snapshots change
watch(
  () => [props.snapshots, props.head],
  () => {
    const { nodes, edges } = buildTreeLayout(props.snapshots);
    flowNodes.value = nodes;
    flowEdges.value = edges;
  },
  { immediate: true, deep: true }
);

// Focus on HEAD node with proper zoom
function focusOnHead() {
  if (!props.head || flowNodes.value.length === 0) {
    fitViewAll();
    return;
  }

  const headNode = flowNodes.value.find((n) => n.id === props.head);
  if (headNode) {
    // Center on HEAD node with zoom 1
    setCenter(headNode.position.x + NODE_WIDTH / 2, headNode.position.y + NODE_HEIGHT / 2, {
      zoom: 1,
      duration: 300,
    });
  } else {
    fitViewAll();
  }
}

// Fit view to show all nodes
function fitViewAll() {
  fitView({ padding: 0.15, duration: 300 });
}

// Handle Vue Flow ready event
function onFlowReady() {
  isReady.value = true;
  // Wait a bit for nodes to be properly positioned
  setTimeout(() => {
    focusOnHead();
  }, 100);
}

// Event handlers
function onApply(date: string) {
  emit('apply', date);
}

function onDelete(date: string) {
  emit('delete', date);
}

function onChangeDescription(date: string) {
  emit('changeDescription', date);
}

function onSetHead(date: string) {
  emit('setHead', date);
}

function onDetach(date: string) {
  emit('detach', date);
}

function onCreateBranch(date: string) {
  emit('createBranch', date);
}
</script>

<template>
  <div class="branch-tree-view">
    <div v-if="snapshots.length === 0" class="empty-state">
      <el-empty :description="$t('manage.no_snapshots')" />
    </div>
    <VueFlow
      v-else
      ref="vueFlowRef"
      v-model:nodes="flowNodes"
      v-model:edges="flowEdges"
      :default-viewport="{ zoom: 1, x: 0, y: 0 }"
      :min-zoom="0.1"
      :max-zoom="2"
      :nodes-draggable="false"
      :nodes-connectable="false"
      :edges-updatable="false"
      class="vue-flow-wrapper"
      @pane-ready="onFlowReady"
    >
      <template #node-snapshot="nodeProps">
        <SnapshotNode
          v-bind="nodeProps"
          @apply="onApply"
          @delete="onDelete"
          @change-description="onChangeDescription"
          @set-head="onSetHead"
          @detach="onDetach"
          @create-branch="onCreateBranch"
        />
      </template>

      <Background
        :variant="BackgroundVariant.Dots"
        pattern-color="#d0d0d0"
        :gap="16"
        :size="1"
        bg-color="#ffffff"
      />

      <!-- Custom controls -->
      <div class="custom-controls">
        <el-tooltip :content="$t('manage.focus_head')" placement="left">
          <el-button :icon="Aim" class="control-btn" @click="focusOnHead" />
        </el-tooltip>
        <el-tooltip :content="$t('manage.fit_view')" placement="left">
          <el-button :icon="FullScreen" class="control-btn" @click="fitViewAll" />
        </el-tooltip>
      </div>
    </VueFlow>
  </div>
</template>

<style scoped>
.branch-tree-view {
  width: 100%;
  height: 100%;
  min-height: 400px;
  border-radius: 8px;
  overflow: hidden;
  position: relative;
}

.empty-state {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  min-height: 300px;
  background: #ffffff;
}

.vue-flow-wrapper {
  width: 100%;
  height: 100%;
  background: #ffffff;
}

.custom-controls {
  position: absolute;
  bottom: 24px;
  right: 24px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  z-index: 10;
}

.custom-controls .control-btn {
  width: 40px;
  height: 40px;
  border-radius: 8px;
  font-size: 18px;
  margin: 0;
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color);
  box-shadow: var(--el-box-shadow-light);
  transition: all 0.2s;
}

.custom-controls .control-btn:hover {
  background: var(--el-color-primary-light-9);
  border-color: var(--el-color-primary);
  color: var(--el-color-primary);
  transform: scale(1.05);
}

:deep(.vue-flow__edge-path) {
  stroke: var(--el-color-primary);
  stroke-width: 2;
}

:deep(.vue-flow__edge.animated path) {
  stroke-dasharray: 5;
  animation: dashdraw 0.5s linear infinite;
}

@keyframes dashdraw {
  from {
    stroke-dashoffset: 10;
  }
  to {
    stroke-dashoffset: 0;
  }
}
</style>
