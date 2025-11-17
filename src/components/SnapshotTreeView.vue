<script lang="ts" setup>
import { computed, ref } from 'vue';
import type { Snapshot } from '../bindings';
import { $t } from '../i18n';

interface Props {
  snapshots: Snapshot[];
  currentHead?: string | null;
}

interface TreeNode extends Snapshot {
  children: TreeNode[];
  level: number;
  x: number;
  y: number;
}

const props = defineProps<Props>();
const emit = defineEmits<{
  apply: [date: string];
  delete: [date: string];
  changeDescribe: [date: string];
  setHead: [date: string];
  detach: [date: string];
}>();

const svgWidth = 1200;
const svgHeight = ref(600);
const nodeRadius = 8;
const levelWidth = 200;
const nodeSpacing = 60;

// Build tree structure
const treeRoots = computed(() => {
  const nodeMap = new Map<string, TreeNode>();
  const roots: TreeNode[] = [];

  // Create all nodes
  props.snapshots.forEach((snapshot) => {
    nodeMap.set(snapshot.date, {
      ...snapshot,
      children: [],
      level: 0,
      x: 0,
      y: 0,
    });
  });

  // Build parent-child relationships
  props.snapshots.forEach((snapshot) => {
    const node = nodeMap.get(snapshot.date)!;
    if (snapshot.parent_id) {
      const parent = nodeMap.get(snapshot.parent_id);
      if (parent) {
        parent.children.push(node);
      } else {
        // Parent not found, treat as root
        roots.push(node);
      }
    } else {
      roots.push(node);
    }
  });

  // Calculate positions
  let globalY = 50;
  const processNode = (node: TreeNode, level: number, baseY: number): number => {
    node.level = level;
    node.x = 100 + level * levelWidth;

    if (node.children.length === 0) {
      node.y = baseY;
      return baseY + nodeSpacing;
    }

    let currentY = baseY;
    node.children.forEach((child) => {
      currentY = processNode(child, level + 1, currentY);
    });

    // Position parent in the middle of children
    const firstChild = node.children[0];
    const lastChild = node.children[node.children.length - 1];
    node.y = (firstChild.y + lastChild.y) / 2;

    return currentY;
  };

  roots.forEach((root) => {
    globalY = processNode(root, 0, globalY);
    globalY += 40; // Extra spacing between trees
  });

  svgHeight.value = Math.max(600, globalY + 50);

  return roots;
});

// Get all nodes in a flat array for rendering
const allNodes = computed(() => {
  const nodes: TreeNode[] = [];
  const traverse = (node: TreeNode) => {
    nodes.push(node);
    node.children.forEach(traverse);
  };
  treeRoots.value.forEach(traverse);
  return nodes;
});

// Get all edges for rendering
const edges = computed(() => {
  const edgeList: Array<{ from: TreeNode; to: TreeNode }> = [];
  const traverse = (node: TreeNode) => {
    node.children.forEach((child) => {
      edgeList.push({ from: node, to: child });
      traverse(child);
    });
  };
  treeRoots.value.forEach(traverse);
  return edgeList;
});

const getNodeClass = (node: TreeNode) => {
  if (props.currentHead === node.date) return 'node-head';
  if (!node.parent_id) return 'node-root';
  return 'node-normal';
};

const getNodeTitle = (node: TreeNode) => {
  const parts = [node.date, node.describe || $t('manage.description')];
  if (props.currentHead === node.date) parts.push(`(${$t('manage.tree_view_head')})`);
  if (!node.parent_id) parts.push(`(${$t('manage.tree_view_root')})`);
  return parts.join(' - ');
};
</script>

<template>
  <div class="tree-view-container">
    <svg :width="svgWidth" :height="svgHeight" class="tree-svg">
      <!-- Draw edges first (so they appear behind nodes) -->
      <g class="edges">
        <line
          v-for="(edge, idx) in edges"
          :key="`edge-${idx}`"
          :x1="edge.from.x"
          :y1="edge.from.y"
          :x2="edge.to.x"
          :y2="edge.to.y"
          class="edge-line"
        />
      </g>

      <!-- Draw nodes -->
      <g class="nodes">
        <g v-for="node in allNodes" :key="node.date" class="node-group">
          <!-- Node circle -->
          <circle
            :cx="node.x"
            :cy="node.y"
            :r="nodeRadius"
            :class="getNodeClass(node)"
            :title="getNodeTitle(node)"
          />

          <!-- Node label -->
          <text :x="node.x + 15" :y="node.y - 10" class="node-label">
            {{ node.describe || node.date.substring(11) }}
          </text>

          <!-- Node date (smaller text below) -->
          <text :x="node.x + 15" :y="node.y + 5" class="node-date">
            {{ node.date }}
          </text>

          <!-- Context menu trigger area (invisible circle for hover) -->
          <circle
            :cx="node.x"
            :cy="node.y"
            :r="nodeRadius * 2"
            class="node-hover-area"
            @contextmenu.prevent="
              (e) => {
                // Show context menu
              }
            "
          >
            <title>{{ getNodeTitle(node) }}</title>
          </circle>
        </g>
      </g>
    </svg>

    <!-- Legend -->
    <div class="legend">
      <div class="legend-item">
        <div class="legend-color node-head"></div>
        <span>{{ $t('manage.tree_view_head') }}</span>
      </div>
      <div class="legend-item">
        <div class="legend-color node-root"></div>
        <span>{{ $t('manage.tree_view_root') }}</span>
      </div>
      <div class="legend-item">
        <div class="legend-color node-normal"></div>
        <span>{{ $t('manage.description') }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.tree-view-container {
  width: 100%;
  overflow-x: auto;
  padding: 20px;
  background-color: var(--el-bg-color);
  border-radius: 8px;
}

.tree-svg {
  display: block;
  border: 1px solid var(--el-border-color);
  background-color: var(--el-fill-color-blank);
  border-radius: 4px;
}

.edge-line {
  stroke: var(--el-border-color);
  stroke-width: 2;
  fill: none;
}

.node-group {
  cursor: pointer;
}

.node-head {
  fill: var(--el-color-success);
  stroke: var(--el-color-success-dark-2);
  stroke-width: 2;
}

.node-root {
  fill: var(--el-color-primary);
  stroke: var(--el-color-primary-dark-2);
  stroke-width: 2;
}

.node-normal {
  fill: var(--el-color-info-light-5);
  stroke: var(--el-color-info);
  stroke-width: 2;
}

.node-hover-area {
  fill: transparent;
  cursor: pointer;
}

.node-hover-area:hover {
  fill: rgba(0, 0, 0, 0.05);
}

.node-label {
  font-size: 14px;
  fill: var(--el-text-color-primary);
  pointer-events: none;
  user-select: none;
}

.node-date {
  font-size: 11px;
  fill: var(--el-text-color-secondary);
  pointer-events: none;
  user-select: none;
}

.legend {
  display: flex;
  gap: 20px;
  margin-top: 15px;
  padding: 10px;
  background-color: var(--el-fill-color-light);
  border-radius: 4px;
}

.legend-item {
  display: flex;
  align-items: center;
  gap: 8px;
}

.legend-color {
  width: 16px;
  height: 16px;
  border-radius: 50%;
  border: 2px solid;
}

.legend-color.node-head {
  background-color: var(--el-color-success);
  border-color: var(--el-color-success-dark-2);
}

.legend-color.node-root {
  background-color: var(--el-color-primary);
  border-color: var(--el-color-primary-dark-2);
}

.legend-color.node-normal {
  background-color: var(--el-color-info-light-5);
  border-color: var(--el-color-info);
}
</style>
