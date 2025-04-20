<script setup lang="ts">
import { ref, computed } from 'vue';
import { $t } from "../i18n";
import { commands } from "../bindings";
import { QuestionFilled } from '@element-plus/icons-vue';

const props = defineProps({
  // 当前路径值，用于在插入变量时保持其他部分不变
  currentPath: {
    type: String,
    default: ''
  },
  // 输入框的引用，用于在插入变量后聚焦
  inputRef: {
    type: Object,
    default: null
  }
});

const emit = defineEmits<{
  (e: 'insert', value: string): void
}>();

// 路径变量分组
const pathVariables = computed(() => [
  {
    group: $t('path_variable.common'),
    variables: [
      { name: 'home', label: $t('path_variable.home'), value: '<home>' },
      { name: 'osUserName', label: $t('path_variable.os_user_name'), value: '<osUserName>' },
    ]
  },
  {
    group: $t('path_variable.windows'),
    variables: [
      { name: 'winAppData', label: $t('path_variable.win_app_data'), value: '<winAppData>' },
      { name: 'winLocalAppData', label: $t('path_variable.win_local_app_data'), value: '<winLocalAppData>' },
      { name: 'winLocalAppDataLow', label: $t('path_variable.win_local_app_data_low'), value: '<winLocalAppDataLow>' },
      { name: 'winDocuments', label: $t('path_variable.win_documents'), value: '<winDocuments>' },
      { name: 'winPublic', label: $t('path_variable.win_public'), value: '<winPublic>' },
      { name: 'winProgramData', label: $t('path_variable.win_program_data'), value: '<winProgramData>' },
      { name: 'winDir', label: $t('path_variable.win_dir'), value: '<winDir>' },
    ]
  },
  {
    group: $t('path_variable.linux'),
    variables: [
      { name: 'xdgData', label: $t('path_variable.xdg_data'), value: '<xdgData>' },
      { name: 'xdgConfig', label: $t('path_variable.xdg_config'), value: '<xdgConfig>' },
    ]
  }
]);

// 插入变量到路径
function insertVariable(variable: string) {
  emit('insert', variable);
}

// 测试路径解析（使用外部路径）
const resolvedPath = ref('');
const isResolving = ref(false);
const hasResolutionError = ref(false);

async function resolvePath() {
  // 使用外部传入的当前路径
  if (!props.currentPath) return;

  isResolving.value = true;
  hasResolutionError.value = false;

  try {
    const result = await commands.resolvePath(props.currentPath);
    if (result.status === "ok") {
      resolvedPath.value = result.data;
    } else {
      hasResolutionError.value = true;
      resolvedPath.value = result.error;
    }
  } catch (e) {
    hasResolutionError.value = true;
    resolvedPath.value = e instanceof Error ? e.message : String(e);
  } finally {
    isResolving.value = false;
  }
}
</script>

<template>
  <div class="path-variable-selector">
    <el-popover placement="bottom" :width="400" trigger="click">
      <template #reference>
        <el-button type="primary" size="small">
          <el-tooltip :content="$t('path_variable.tooltip')" placement="top">
            {{ $t('path_variable.insert_variable') }}
          </el-tooltip>
        </el-button>
      </template>

      <div class="variable-list">
        <div v-for="group in pathVariables" :key="group.group" class="variable-group">
          <div class="group-title">{{ group.group }}</div>
          <div class="group-variables">
            <el-link v-for="variable in group.variables" :key="variable.name" size="small"
              @click="insertVariable(variable.value)" :title="variable.label">
              {{ variable.value }}
            </el-link>
          </div>
        </div>

        <div class="path-tester">
          <div class="tester-title">{{ $t('path_variable.test_resolution') }}</div>
          <div class="resolve-button-container">
            <el-button @click="resolvePath" :loading="isResolving" size="small">
              {{ $t('path_variable.resolve') }}
            </el-button>
          </div>

          <div v-if="resolvedPath" class="resolved-path" :class="{ 'error': hasResolutionError }">
            {{ resolvedPath }}
          </div>
        </div>
      </div>
    </el-popover>

  </div>
</template>

<style scoped>
.path-variable-selector {
  display: flex;
  align-items: center;
  gap: 8px;
}

.variable-list {
  max-height: 400px;
  overflow-y: auto;
}

.variable-group {
  margin-bottom: 12px;
}

.group-title {
  font-weight: bold;
  margin-bottom: 8px;
  color: var(--el-text-color-primary);
}

.group-variables {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.path-tester {
  margin-top: 16px;
  padding-top: 16px;
  border-top: 1px solid var(--el-border-color-lighter);
}

.tester-title {
  font-weight: bold;
  margin-bottom: 8px;
  color: var(--el-text-color-primary);
}

.resolved-path {
  margin-top: 8px;
  padding: 8px;
  background-color: var(--el-fill-color-lighter);
  border-radius: 4px;
  word-break: break-all;
}

.resolved-path.error {
  color: var(--el-color-danger);
}

.resolve-button-container {
  margin-bottom: 8px;
}
</style>