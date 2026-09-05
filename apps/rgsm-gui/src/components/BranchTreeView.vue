<script setup lang="ts">
import { ref, watch } from 'vue';
import { VueFlow, useVueFlow } from '@vue-flow/core';
import { Background, BackgroundVariant } from '@vue-flow/background';
import type { Edge, Node } from '@vue-flow/core';
import SnapshotNode from './SnapshotNode.vue';
import type { Snapshot } from '../api/commands';
import { $t } from '../i18n';
import { Crosshair, Inbox, Maximize } from '@lucide/vue';
import { KButton, KTooltip } from '../ui/kit';
import { compareSnapshotTime, snapshotDeviceName } from '../utils/snapshotPresentation';

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
  devices?: Record<string, { name: string }>;
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
    children.sort(compareSnapshotTime);
  }

  const snapshotDates = new Set(snapshots.map((snapshot) => snapshot.date));
  const rootSnapshots = snapshots.filter(
    (snapshot) =>
      snapshot.parent === null ||
      snapshot.parent === undefined ||
      !snapshotDates.has(snapshot.parent)
  );
  rootSnapshots.sort(compareSnapshotTime);

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
        creatorLabel: $t('manage.snapshot_creator', {
          device:
            snapshotDeviceName(node.snapshot.device_id, props.devices) ??
            $t('manage.unknown_snapshot_device'),
        }),
        isHead: headMarkers.length > 0,
        isCurrentHead: node.snapshot.date === props.currentHead,
        isRoot: node.parent === null,
        headMarkers,
        canEditDescription:
          !props.editableDates || props.editableDates.includes(node.snapshot.date),
      },
    });

    if (node.parent) {
      const isHeadEdge = node.snapshot.date === props.currentHead;
      edges.push({
        id: `${node.snapshot.date}-${node.parent.snapshot.date}`,
        source: node.snapshot.date,
        target: node.parent.snapshot.date,
        type: 'smoothstep',
        style: {
          // 琥珀只给 HEAD 路径,其余边用中性 hairline
          stroke: isHeadEdge ? 'var(--accent)' : 'var(--border-strong)',
          strokeWidth: isHeadEdge ? 2 : 1.5,
        },
        animated: isHeadEdge,
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
  () => [props.snapshots, props.currentHead, props.deviceHeads, props.devices],
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
    <div
      v-if="snapshots.length === 0"
      class="flex h-full min-h-[300px] flex-col items-center justify-center gap-2 text-text-dim"
    >
      <Inbox :size="28" aria-hidden="true" />
      <p class="text-sm">{{ $t('manage.no_snapshots') }}</p>
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
        <KTooltip :content="$t('manage.focus_head')" side="left">
          <KButton :aria-label="$t('manage.focus_head')" @click="focusOnHead">
            <Crosshair :size="15" aria-hidden="true" />
          </KButton>
        </KTooltip>
        <KTooltip :content="$t('manage.fit_view')" side="left">
          <KButton :aria-label="$t('manage.fit_view')" @click="fitViewAll">
            <Maximize :size="15" aria-hidden="true" />
          </KButton>
        </KTooltip>
      </div>
    </VueFlow>
  </div>
</template>

<style scoped>
.branch-tree-view {
  width: 100%;
  height: 100%;
  border-radius: var(--radius-md);
  overflow: hidden;
  position: relative;
}

.vue-flow-wrapper {
  width: 100%;
  height: 100%;
  background: transparent;
}

:deep(.vue-flow__background pattern circle) {
  fill: var(--border);
}

.custom-controls {
  position: absolute;
  bottom: 20px;
  right: 20px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  z-index: 10;
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
