<script setup lang="ts">
import { ref, watch } from 'vue';
import { VueFlow, useVueFlow } from '@vue-flow/core';
import { Background, BackgroundVariant } from '@vue-flow/background';
import type { Edge, Node } from '@vue-flow/core';
import SnapshotNode from './SnapshotNode.vue';
import type { Snapshot } from '../api/commands';
import { $t } from '../i18n';
import { Aim, FullScreen } from '@element-plus/icons-vue';

import '@vue-flow/core/dist/style.css';
import '@vue-flow/core/dist/theme-default.css';

interface DeviceHeadMarker {
  deviceId: string;
  date: string;
  label: string;
  isCurrentDevice: boolean;
  tooltip: string;
}

interface Props {
  snapshots: Snapshot[];
  currentHead: string | null;
  deviceHeads: DeviceHeadMarker[];
  editableDates?: string[];
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
  number: number;
  parent: TreeNode | null;
}

function buildTreeLayout(snapshots: Snapshot[]): { nodes: Node[]; edges: Edge[] } {
  if (snapshots.length === 0) {
    return { nodes: [], edges: [] };
  }

  const childrenMap = new Map<string, Snapshot[]>();

  for (const snapshot of snapshots) {
    const parentKey = snapshot.parent ?? '__root__';
    if (!childrenMap.has(parentKey)) {
      childrenMap.set(parentKey, []);
    }
    childrenMap.get(parentKey)!.push(snapshot);
  }

  for (const children of childrenMap.values()) {
    children.sort((a, b) => a.date.localeCompare(b.date));
  }

  const snapshotDates = new Set(snapshots.map((snapshot) => snapshot.date));
  const rootSnapshots = snapshots.filter(
    (snapshot) =>
      snapshot.parent === null ||
      snapshot.parent === undefined ||
      !snapshotDates.has(snapshot.parent)
  );
  rootSnapshots.sort((a, b) => a.date.localeCompare(b.date));

  function buildTreeNode(snapshot: Snapshot, parent: TreeNode | null, depth: number): TreeNode {
    const node: TreeNode = {
      snapshot,
      children: [],
      x: 0,
      y: depth,
      depth,
      number: 0,
      parent,
    };

    const children = childrenMap.get(snapshot.date) || [];
    node.children = children.map((child, index) => {
      const childNode = buildTreeNode(child, node, depth + 1);
      childNode.number = index;
      return childNode;
    });

    return node;
  }

  const trees: TreeNode[] = rootSnapshots.map((snapshot, index) => {
    const node = buildTreeNode(snapshot, null, 0);
    node.number = index;
    return node;
  });

  function layoutTree(node: TreeNode, minX: number): number {
    if (node.children.length === 0) {
      node.x = minX;
      return minX + 1;
    }

    let nextX = minX;
    for (const child of node.children) {
      nextX = layoutTree(child, nextX);
    }

    const firstChild = node.children[0]!;
    const lastChild = node.children[node.children.length - 1]!;
    node.x = (firstChild.x + lastChild.x) / 2;

    return nextX;
  }

  function getMaxDepth(node: TreeNode): number {
    if (node.children.length === 0) return node.depth;
    return Math.max(...node.children.map(getMaxDepth));
  }

  let currentMinX = 0;
  let globalMaxDepth = 0;

  for (const tree of trees) {
    currentMinX = layoutTree(tree, currentMinX) + 1;
    globalMaxDepth = Math.max(globalMaxDepth, getMaxDepth(tree));
  }

  const nodes: Node[] = [];
  const edges: Edge[] = [];

  function collectNodesAndEdges(node: TreeNode) {
    const posX = node.x * (NODE_WIDTH + HORIZONTAL_GAP);
    const posY = (globalMaxDepth - node.depth) * (NODE_HEIGHT + VERTICAL_GAP);
    const headMarkers = props.deviceHeads
      .filter((marker) => marker.date === node.snapshot.date)
      .map((marker) => ({
        deviceId: marker.deviceId,
        label: marker.label,
        isCurrentDevice: marker.isCurrentDevice,
        tooltip: marker.tooltip,
      }));

    nodes.push({
      id: node.snapshot.date,
      type: 'snapshot',
      position: { x: posX, y: posY },
      data: {
        snapshot: node.snapshot,
        isHead: headMarkers.length > 0,
        isCurrentHead: node.snapshot.date === props.currentHead,
        isRoot: node.parent === null,
        headMarkers,
        canEditDescription:
          !props.editableDates || props.editableDates.includes(node.snapshot.date),
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
        animated: node.snapshot.date === props.currentHead,
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

watch(
  () => [props.snapshots, props.currentHead, props.deviceHeads],
  () => {
    const { nodes, edges } = buildTreeLayout(props.snapshots);
    flowNodes.value = nodes;
    flowEdges.value = edges;
  },
  { immediate: true, deep: true }
);

function focusOnHead() {
  if (!props.currentHead || flowNodes.value.length === 0) {
    fitViewAll();
    return;
  }

  const headNode = flowNodes.value.find((node: { id: string }) => node.id === props.currentHead);
  if (!headNode) {
    fitViewAll();
    return;
  }

  setCenter(headNode.position.x + NODE_WIDTH / 2, headNode.position.y + NODE_HEIGHT / 2, {
    zoom: 1,
    duration: 300,
  });
}

function fitViewAll() {
  fitView({ padding: 0.15, duration: 300 });
}

function onFlowReady() {
  setTimeout(() => {
    focusOnHead();
  }, 100);
}
</script>

<template>
  <div class="branch-tree-view">
    <div v-if="snapshots.length === 0" class="empty-state">
      <el-empty :description="$t('manage.no_snapshots')" />
    </div>
    <VueFlow
      v-else
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
          @apply="emit('apply', $event)"
          @delete="emit('delete', $event)"
          @change-description="emit('changeDescription', $event)"
          @set-head="emit('setHead', $event)"
          @detach="emit('detach', $event)"
          @create-branch="emit('createBranch', $event)"
        />
      </template>

      <Background
        :variant="BackgroundVariant.Dots"
        :gap="20"
        :size="1"
        class="vue-flow-background"
      />

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
  background: var(--el-bg-color);
}

.vue-flow-wrapper {
  width: 100%;
  height: 100%;
  background: var(--el-bg-color);
}

:deep(.vue-flow-background) {
  background-color: var(--el-bg-color);
}

:deep(.vue-flow__background pattern circle) {
  fill: var(--el-border-color-lighter);
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
