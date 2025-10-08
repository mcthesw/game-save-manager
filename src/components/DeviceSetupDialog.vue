<template>
  <el-dialog
    :title="$t('device_setup.title')"
    :model-value="modelValue"
    @update:model-value="$emit('update:modelValue', $event)"
    width="500px"
    :close-on-click-modal="false"
    :close-on-press-escape="false"
    :show-close="false"
  >
    <el-form :model="form" label-position="top">
      <!-- 设备名称输入 -->
      <el-form-item :label="$t('device_setup.device_name')">
        <el-input v-model="form.deviceName" :placeholder="$t('device_setup.device_name_placeholder')" />
      </el-form-item>

      <!-- 如果有其他设备，显示导入选项 -->
      <el-form-item v-if="otherDevices.length > 0" :label="$t('device_setup.import_from')">
        <el-select v-model="form.importFromDeviceId" clearable :placeholder="$t('device_setup.select_device')">
          <el-option
            v-for="device in otherDevices"
            :key="device.id"
            :label="device.name"
            :value="device.id"
          />
        </el-select>
      </el-form-item>
    </el-form>

    <template #footer>
      <div class="dialog-footer">
        <el-button type="primary" @click="confirm">{{ $t('common.confirm') }}</el-button>
      </div>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
import { ref, defineEmits, watch } from 'vue';
import { $t } from '../i18n';
import type { Device } from '../bindings';

const props = defineProps({
  modelValue: {
    type: Boolean,
    required: true
  },
  defaultDeviceName: {
    type: String,
    default: ''
  },
  otherDevices: {
    type: Array as () => Device[],
    default: () => []
  }
});

const emits = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'confirm', deviceName: string, importFromDeviceId?: string): void;
}>();

// 表单数据
const form = ref({
  deviceName: props.defaultDeviceName,
  importFromDeviceId: ''
});

// 监听默认设备名变化
watch(() => props.defaultDeviceName, (newValue) => {
  form.value.deviceName = newValue;
});

// 确认按钮
function confirm() {
  if (!form.value.deviceName.trim()) {
    form.value.deviceName = props.defaultDeviceName;
  }
  
  emits('confirm', form.value.deviceName, form.value.importFromDeviceId || undefined);
  emits('update:modelValue', false);
}
</script>

<style scoped>
.dialog-footer {
  display: flex;
  justify-content: flex-end;
}
</style>
