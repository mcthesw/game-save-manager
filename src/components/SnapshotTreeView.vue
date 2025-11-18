<script lang="ts" setup>
import { onMounted, ref, watch, nextTick } from 'vue';
import { createGitgraph, templateExtend, TemplateName } from '@gitgraph/js';
import type { Snapshot } from '../bindings';
import { $t } from '../i18n';

interface Props {
  snapshots: Snapshot[];
  currentHead?: string | null;
}

const props = defineProps<Props>();
const emit = defineEmits<{
  apply: [date: string];
  delete: [date: string];
  changeDescribe: [date: string];
  setHead: [date: string];
  detach: [date: string];
}>();

const graphContainer = ref<HTMLElement | null>(null);
const selectedNode = ref<Snapshot | null>(null);
const showActions = ref(false);

// Render the gitgraph
const renderGraph = async () => {
  await nextTick();
  if (!graphContainer.value) return;

  // Clear previous graph
  graphContainer.value.innerHTML = '';

  if (props.snapshots.length === 0) {
    graphContainer.value.innerHTML =
      '<p style="text-align: center; padding: 40px;">No snapshots available</p>';
    return;
  }

  // Create custom template
  const customTemplate = templateExtend(TemplateName.Metro, {
    colors: ['#5865F2', '#57F287', '#FEE75C', '#ED4245', '#EB459E', '#F26522', '#1ABC9C'],
    branch: {
      lineWidth: 4,
      spacing: 60,
      label: {
        display: false,
      },
    },
    commit: {
      spacing: 70,
      dot: {
        size: 12,
      },
      message: {
        displayAuthor: false,
        displayHash: false,
        font: 'normal 14px -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
      },
    },
  });

  const gitgraph = createGitgraph(graphContainer.value, {
    template: customTemplate,
    orientation: 'vertical-reverse',
    mode: 'compact',
  });

  // Sort snapshots by date (oldest first)
  const sortedSnapshots = [...props.snapshots].sort((a, b) => a.date.localeCompare(b.date));

  // Build parent-child map
  const childrenMap = new Map<string, Snapshot[]>();
  sortedSnapshots.forEach((snapshot) => {
    if (snapshot.parent_id) {
      if (!childrenMap.has(snapshot.parent_id)) {
        childrenMap.set(snapshot.parent_id, []);
      }
      childrenMap.get(snapshot.parent_id)!.push(snapshot);
    }
  });

  // Find roots
  const roots = sortedSnapshots.filter((s) => !s.parent_id);

  // Branch map to track created branches
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const branchMap = new Map<string, any>();

  // Helper to get node color
  const getNodeColor = (snapshot: Snapshot) => {
    if (props.currentHead === snapshot.date) return '#57F287'; // HEAD - green
    if (!snapshot.parent_id) return '#5865F2'; // Root - blue
    return '#99AAB5'; // Normal - gray
  };

  // Helper to format display text
  const formatNodeText = (snapshot: Snapshot) => {
    const time = snapshot.date.substring(11, 19);
    const desc = snapshot.describe || 'No description';
    let badge = '';

    if (props.currentHead === snapshot.date) {
      badge = ' 🎯 HEAD';
    } else if (!snapshot.parent_id) {
      badge = ' 🌟 ROOT';
    }

    return `${time} - ${desc}${badge}`;
  };

  // Recursive function to render tree
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const renderTree = (snapshot: Snapshot, parentBranch: any = null, branchIndex: number = 0) => {
    const color = getNodeColor(snapshot);

    let branch;
    if (parentBranch) {
      // Create child branch
      const branchName = `branch-${snapshot.date}`;
      branch = parentBranch.branch({
        name: branchName,
        style: {
          color: color,
        },
      });
    } else {
      // Create root branch
      const branchName = `root-${branchIndex}`;
      branch = gitgraph.branch({
        name: branchName,
        style: {
          color: color,
        },
      });
    }

    // Store branch
    branchMap.set(snapshot.date, branch);

    // Create commit with click handler
    const commitOptions = {
      subject: formatNodeText(snapshot),
      hash: snapshot.date.substring(11, 19),
      style: {
        dot: {
          color: color,
          size: 12,
          strokeWidth: 3,
          strokeColor: '#2C2F33',
        },
        message: {
          color: 'var(--el-text-color-primary)',
        },
      },
      onClick: () => {
        selectedNode.value = snapshot;
        showActions.value = true;
      },
    };

    branch.commit(commitOptions);

    // Render children
    const children = childrenMap.get(snapshot.date) || [];
    children.forEach((child) => {
      renderTree(child, branch, branchIndex);
    });
  };

  // Render all root trees
  roots.forEach((root, index) => {
    renderTree(root, null, index);
  });
};

// Initialize graph
onMounted(() => {
  renderGraph();
});

// Re-render when data changes
watch(
  () => [props.snapshots, props.currentHead],
  () => {
    renderGraph();
  },
  { deep: true }
);

// Action handlers
const handleApply = () => {
  if (selectedNode.value) {
    emit('apply', selectedNode.value.date);
    showActions.value = false;
  }
};

const handleDelete = () => {
  if (selectedNode.value) {
    emit('delete', selectedNode.value.date);
    showActions.value = false;
  }
};

const handleChangeDescribe = () => {
  if (selectedNode.value) {
    emit('changeDescribe', selectedNode.value.date);
    showActions.value = false;
  }
};

const handleSetHead = () => {
  if (selectedNode.value) {
    emit('setHead', selectedNode.value.date);
    showActions.value = false;
  }
};

const handleDetach = () => {
  if (selectedNode.value) {
    emit('detach', selectedNode.value.date);
    showActions.value = false;
  }
};
</script>

<template>
  <div class="tree-view-container">
    <!-- Gitgraph container -->
    <div ref="graphContainer" class="gitgraph-container"></div>

    <!-- Action dialog -->
    <el-dialog
      v-model="showActions"
      :title="$t('manage.description')"
      width="400px"
      :append-to-body="true"
    >
      <div v-if="selectedNode" class="node-details">
        <div class="detail-row">
          <strong>{{ $t('manage.save_date') }}:</strong>
          <span>{{ selectedNode.date }}</span>
        </div>
        <div class="detail-row">
          <strong>{{ $t('manage.description') }}:</strong>
          <span>{{ selectedNode.describe || 'N/A' }}</span>
        </div>
        <div class="detail-row">
          <strong>{{ $t('manage.size') }}:</strong>
          <span>{{
            selectedNode.size ? (selectedNode.size / 1024 / 1024).toFixed(2) + ' MB' : 'N/A'
          }}</span>
        </div>
      </div>

      <template #footer>
        <div class="dialog-footer">
          <el-button type="primary" @click="handleApply">
            {{ $t('manage.apply') }}
          </el-button>
          <el-button @click="handleChangeDescribe">
            {{ $t('manage.change_describe') }}
          </el-button>
          <el-button @click="handleSetHead">
            {{ $t('manage.set_as_head') }}
          </el-button>
          <el-button @click="handleDetach">
            {{ $t('manage.detach_from_parent') }}
          </el-button>
          <el-button type="danger" @click="handleDelete">
            {{ $t('manage.delete') }}
          </el-button>
        </div>
      </template>
    </el-dialog>

    <!-- Info panel -->
    <div class="info-panel">
      <div class="info-row">
        <strong>{{ $t('manage.tree_view_head') }}:</strong>
        <span>{{ currentHead || $t('manage.no_backup_error') }}</span>
      </div>
      <div class="info-row">
        <span class="info-hint"
          >💡 {{ $t('manage.description') }}: Click on any node to see available actions</span
        >
      </div>
    </div>

    <!-- Legend -->
    <div class="legend">
      <div class="legend-item">
        <div class="legend-color" style="background-color: #57f287; border-color: #2c2f33"></div>
        <span>🎯 {{ $t('manage.tree_view_head') }}</span>
      </div>
      <div class="legend-item">
        <div class="legend-color" style="background-color: #5865f2; border-color: #2c2f33"></div>
        <span>🌟 {{ $t('manage.tree_view_root') }}</span>
      </div>
      <div class="legend-item">
        <div class="legend-color" style="background-color: #99aab5; border-color: #2c2f33"></div>
        <span>{{ $t('manage.description') }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.tree-view-container {
  width: 100%;
  padding: 20px;
  background-color: var(--el-bg-color);
  border-radius: 8px;
}

.gitgraph-container {
  min-height: 400px;
  max-height: 800px;
  overflow: auto;
  background-color: var(--el-fill-color-blank);
  border: 1px solid var(--el-border-color);
  border-radius: 8px;
  padding: 30px;
}

.node-details {
  padding: 10px 0;
}

.detail-row {
  display: flex;
  justify-content: space-between;
  margin-bottom: 12px;
  padding: 8px;
  background-color: var(--el-fill-color-light);
  border-radius: 4px;
}

.detail-row strong {
  color: var(--el-text-color-primary);
  margin-right: 10px;
}

.detail-row span {
  color: var(--el-text-color-regular);
  text-align: right;
  flex: 1;
}

.dialog-footer {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  justify-content: center;
}

.dialog-footer .el-button {
  flex: 1 1 auto;
  min-width: 120px;
}

.info-panel {
  margin-top: 20px;
  padding: 15px;
  background-color: var(--el-fill-color-light);
  border-radius: 8px;
  border-left: 4px solid var(--el-color-primary);
}

.info-row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 8px;
}

.info-row:last-child {
  margin-bottom: 0;
}

.info-row strong {
  font-size: 14px;
  color: var(--el-text-color-primary);
}

.info-row span {
  font-size: 14px;
  color: var(--el-text-color-regular);
}

.info-hint {
  font-size: 13px;
  color: var(--el-text-color-secondary);
  font-style: italic;
}

.legend {
  display: flex;
  gap: 20px;
  margin-top: 20px;
  padding: 15px;
  background-color: var(--el-fill-color-light);
  border-radius: 8px;
  flex-wrap: wrap;
}

.legend-item {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 14px;
  color: var(--el-text-color-primary);
}

.legend-color {
  width: 24px;
  height: 24px;
  border-radius: 12px;
  border: 3px solid;
}

/* Gitgraph custom styling */
:deep(.gitgraph) {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
}

:deep(.gitgraph-commit-message) {
  fill: var(--el-text-color-primary);
  cursor: pointer;
}

:deep(.gitgraph-dot) {
  cursor: pointer;
}

:deep(.gitgraph-commit-message:hover) {
  opacity: 0.8;
}
</style>
