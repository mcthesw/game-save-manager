<script lang="ts" setup>
import { ref } from 'vue';
import { $t } from '../i18n';

const props = defineProps<{
  modelValue: boolean;
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void;
  (e: 'import'): void;
  (e: 'skip'): void;
}>();

const dialogVisible = ref(props.modelValue);

function handleImport() {
  emit('import');
  dialogVisible.value = false;
  emit('update:modelValue', false);
}

function handleSkip() {
  emit('skip');
  dialogVisible.value = false;
  emit('update:modelValue', false);
}

// Watch for changes in modelValue
watch(() => props.modelValue, (newVal) => {
  dialogVisible.value = newVal;
});

// Update parent when dialog closes
watch(dialogVisible, (newVal) => {
  if (!newVal) {
    emit('update:modelValue', false);
  }
});
</script>

<template>
  <el-dialog
    v-model="dialogVisible"
    :title="$t('steam_import.first_time_title')"
    width="500px"
    :close-on-click-modal="false"
    :close-on-press-escape="false"
    :show-close="false"
  >
    <div class="dialog-content">
      <p>{{ $t('steam_import.first_time_message') }}</p>
    </div>
    
    <template #footer>
      <div class="dialog-footer">
        <el-button @click="handleSkip">
          {{ $t('steam_import.skip') }}
        </el-button>
        <el-button type="primary" @click="handleImport">
          {{ $t('steam_import.import_now') }}
        </el-button>
      </div>
    </template>
  </el-dialog>
</template>

<style scoped>
.dialog-content {
  padding: 20px 0;
}

.dialog-content p {
  font-size: 16px;
  line-height: 1.6;
  margin: 0;
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
}
</style>
