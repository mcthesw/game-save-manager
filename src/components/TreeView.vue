<script lang="ts" setup>
import { ref, watch } from 'vue';
import { VueFlow, type Node, type Edge } from '@vue-flow/core';
import { Background } from '@vue-flow/background';
import { Controls } from '@vue-flow/controls';
import dagre from 'dagre';
import SnapshotNode from './SnapshotNode.vue';
import type { Snapshot } from '../bindings';
import '@vue-flow/core/dist/style.css';
import '@vue-flow/controls/dist/style.css';
import '@vue-flow/core/dist/theme-default.css';

const props = defineProps<{
  snapshots: Snapshot[];
  headDate: string | undefined;
  onRestore: (date: string) => void;
  onDelete: (date: string) => void;
  onEditDescribe: (date: string) => void;
  onDetach: (date: string) => void;
}>();

const nodes = ref<Node[]>([]);
const edges = ref<Edge[]>([]);
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const vueFlowInstance = ref<any>(null);

// Layout function
const layout = (nodes: Node[], edges: Edge[]) => {
  const g = new dagre.graphlib.Graph();
  g.setGraph({ rankdir: 'BT', align: 'DL', nodesep: 100, ranksep: 100 }); // Bottom-to-Top
  g.setDefaultEdgeLabel(() => ({}));

  nodes.forEach((node) => g.setNode(node.id, { width: 220, height: 80 }));
  edges.forEach((edge) => g.setEdge(edge.source, edge.target));

  dagre.layout(g);

  return nodes.map((node) => {
    const pos = g.node(node.id);
    return { ...node, position: { x: pos.x, y: pos.y } };
  });
};

// Convert snapshots to graph elements
const processData = () => {
    const newNodes: Node[] = [];
    const newEdges: Edge[] = [];

    props.snapshots.forEach(s => {
        newNodes.push({
            id: s.date,
            type: 'snapshot',
            data: {
                snapshot: s,
                isHead: s.date === props.headDate,
                onRestore: props.onRestore,
                onDelete: props.onDelete,
                onEditDescribe: props.onEditDescribe,
                onDetach: props.onDetach
            },
            position: { x: 0, y: 0 } // placeholder
        });

        if (s.parent) {
            newEdges.push({
                id: `e-${s.parent}-${s.date}`,
                source: s.parent,
                target: s.date,
                type: 'smoothstep', // smoothstep looks better for tree
                animated: false,
                style: { stroke: 'var(--el-border-color-darker)', strokeWidth: 2 }
            });
        }
    });

    // Calculate layout
    if (newNodes.length > 0) {
        nodes.value = layout(newNodes, newEdges);
        edges.value = newEdges;
    } else {
        nodes.value = [];
        edges.value = [];
    }
};

watch(() => props.snapshots, () => {
    processData();
    // After data change, we might want to re-fit or maintain position?
    // User said: "Enter automatically zoom to appropriate size (HEAD)".
    // Maybe we shouldn't move view on every update, but initially yes.
}, { deep: true, immediate: true });

watch(() => props.headDate, (newVal) => {
    // Update isHead status without full re-layout
    nodes.value = nodes.value.map(n => ({
        ...n,
        data: { ...n.data, isHead: n.id === newVal }
    }));
});

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const onPaneReady = (instance: any) => {
    vueFlowInstance.value = instance;
    setTimeout(() => {
        centerOnHead();
    }, 100);
};

const centerOnHead = () => {
    if (!vueFlowInstance.value) return;

    if (props.headDate) {
         // Find node by id
         const node = vueFlowInstance.value.findNode(props.headDate);
         if (node) {
             vueFlowInstance.value.fitView({ nodes: [node.id], maxZoom: 1, duration: 800 });
         } else {
             vueFlowInstance.value.fitView({ maxZoom: 1, duration: 800 });
         }
    } else {
         vueFlowInstance.value.fitView({ maxZoom: 1, duration: 800 });
    }
};

// Expose centerOnHead for parent to call if needed
defineExpose({ centerOnHead });

</script>

<template>
  <div class="tree-view-container">
    <VueFlow
      v-model:nodes="nodes"
      v-model:edges="edges"
      :min-zoom="0.1"
      :max-zoom="4"
      @pane-ready="onPaneReady"
    >
      <template #node-snapshot="nodeProps">
        <SnapshotNode v-bind="nodeProps" />
      </template>

      <Background pattern-color="#aaa" :gap="16" />
      <Controls />
    </VueFlow>
  </div>
</template>

<style scoped>
.tree-view-container {
    height: 600px; /* Or calculated height */
    width: 100%;
    border: 1px solid var(--el-border-color);
    border-radius: 4px;
    background-color: var(--el-bg-color-page);
}
</style>
