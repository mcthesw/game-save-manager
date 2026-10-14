<script setup lang="ts">
import { Download, MonitorSmartphone } from '@lucide/vue';

import type { Device } from '../api/commands';
import CloudDeviceProfilesPanel from './CloudDeviceProfilesPanel.vue';
import { $t } from '../i18n';
import { KButton, KInput } from '../ui/kit';

const props = defineProps<{
  currentDevice: Device;
  otherDevices: Device[];
  /** Cloud device profiles only exist in the V2 Cloud Library generation. */
  showCloudProfiles: boolean;
}>();

const emit = defineEmits<{
  rename: [];
  importPaths: [deviceId: string];
  deleteDevice: [deviceId: string];
  refreshed: [];
}>();

const deviceName = defineModel<string>('deviceName', { required: true });
</script>

<template>
  <section>
    <div class="mb-3 flex items-center gap-2 border-b border-border pb-2">
      <MonitorSmartphone :size="15" class="text-text-dim" aria-hidden="true" />
      <h2 class="text-sm font-semibold text-text">{{ $t('settings.device_settings') }}</h2>
    </div>

    <h3 class="mb-2 text-xs font-medium text-text">{{ $t('settings.current_device') }}</h3>
    <div class="mb-1">
      <div class="mb-1 block text-xs text-text-dim">{{ $t('settings.device_name') }}</div>
      <KInput
        v-model="deviceName"
        class="w-full"
        :aria-label="$t('settings.device_name')"
        @change="emit('rename')"
      />
    </div>
    <div class="mb-4">
      <div class="mb-1 block text-xs text-text-dim">{{ $t('settings.device_id') }}</div>
      <KInput
        :model-value="props.currentDevice.id"
        class="w-full"
        mono
        disabled
        :aria-label="$t('settings.device_id')"
      />
    </div>

    <CloudDeviceProfilesPanel
      v-if="props.showCloudProfiles"
      class="mt-5"
      @reused="emit('refreshed')"
    />

    <h3 class="mb-2 mt-5 text-xs font-medium text-text">{{ $t('settings.other_devices') }}</h3>
    <div class="flex flex-col">
      <div
        v-for="device in props.otherDevices"
        :key="device.id"
        class="flex items-center justify-between gap-3 border-t border-border py-2"
      >
        <div class="min-w-0">
          <div class="truncate text-sm text-text">{{ device.name }}</div>
          <div class="truncate font-mono text-[11px] text-text-dim">{{ device.id }}</div>
        </div>
        <div class="flex shrink-0 gap-1.5">
          <KButton size="sm" @click="emit('importPaths', device.id)">
            <template #icon><Download :size="13" aria-hidden="true" /></template>
            {{ $t('settings.import_paths') }}
          </KButton>
          <KButton size="sm" variant="danger" @click="emit('deleteDevice', device.id)">
            {{ $t('settings.delete_device') }}
          </KButton>
        </div>
      </div>
      <p v-if="props.otherDevices.length === 0" class="py-2 text-xs text-text-dim">-</p>
    </div>
  </section>
</template>
