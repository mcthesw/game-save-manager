<template>
  <KDialog
    :open="modelValue"
    :title="$t('device_setup.title')"
    :width="500"
    :dismissable="false"
    @update:open="$emit('update:modelValue', $event)"
  >
    <div class="flex flex-col gap-3">
      <!-- 设备名称输入 -->
      <div>
        <div class="mb-1 block text-xs text-text-dim">{{ $t('device_setup.device_name') }}</div>
        <KInput
          v-model="form.deviceName"
          class="w-full"
          :placeholder="$t('device_setup.device_name_placeholder')"
          :aria-label="$t('device_setup.device_name')"
        />
      </div>

      <!-- 如果有其他设备，显示导入选项 -->
      <div v-if="otherDevices.length > 0">
        <div class="mb-1 block text-xs text-text-dim">{{ $t('device_setup.import_from') }}</div>
        <KSelect
          v-model="form.importFromDeviceId"
          class="w-full"
          clearable
          :options="deviceOptions"
          :placeholder="$t('device_setup.select_device')"
          :aria-label="$t('device_setup.import_from')"
        />
      </div>
    </div>

    <template #footer>
      <KButton variant="primary" @click="confirm">{{ $t('common.confirm') }}</KButton>
    </template>
  </KDialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { $t } from '../i18n';
import type { Device } from '../api/commands';
import { KButton, KDialog, KInput, KSelect } from '../ui/kit';

const props = defineProps({
  modelValue: {
    type: Boolean,
    required: true,
  },
  defaultDeviceName: {
    type: String,
    default: '',
  },
  otherDevices: {
    type: Array as () => Device[],
    default: () => [],
  },
});

const emits = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'confirm', deviceName: string, importFromDeviceId?: string): void;
}>();

// 表单数据
const form = ref({
  deviceName: props.defaultDeviceName,
  importFromDeviceId: '',
});

const deviceOptions = computed(() =>
  props.otherDevices.map((device) => ({ value: device.id, label: device.name }))
);

// 监听默认设备名变化
watch(
  () => props.defaultDeviceName,
  (newValue) => {
    form.value.deviceName = newValue;
  }
);

// 确认按钮
function confirm() {
  if (!form.value.deviceName.trim()) {
    form.value.deviceName = props.defaultDeviceName;
  }

  emits('confirm', form.value.deviceName, form.value.importFromDeviceId || undefined);
  emits('update:modelValue', false);
}
</script>
