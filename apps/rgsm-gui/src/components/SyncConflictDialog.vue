<script setup lang="ts">
import { computed } from 'vue';
import { $t } from '../i18n';
import type { ConflictResolution, GameSyncState } from '../api/commands';
import { KButton, KDialog } from '../ui/kit';

const props = defineProps<{
  modelValue: boolean;
  gameName: string;
  state?: GameSyncState | null;
  currentDeviceId?: string;
  resolving?: boolean;
}>();

const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'resolve', resolution: ConflictResolution): void;
}>();

const visible = computed({
  get: () => props.modelValue,
  set: (value: boolean) => emit('update:modelValue', value),
});

function snapshotLabel(value?: string | null): string {
  return value || $t('sync_settings.conflict.no_snapshot');
}

function formatTime(value?: string | null): string {
  if (!value) return $t('sync_settings.conflict.not_synced');
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

function decideLater() {
  visible.value = false;
}
</script>

<template>
  <KDialog v-model:open="visible" :title="$t('sync_settings.conflict.title')" :width="520">
    <p class="mb-4 text-sm leading-relaxed text-text-dim">
      {{ $t('sync_settings.conflict.summary', { game: gameName }) }}
    </p>

    <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
      <section class="rounded-md border border-border bg-surface-2 p-3">
        <h4 class="mb-2.5 text-sm font-medium text-text">
          {{ $t('sync_settings.conflict.local_title') }}
        </h4>
        <dl class="flex flex-col gap-2">
          <div class="flex flex-col gap-0.5">
            <dt class="text-xs text-text-dim">{{ $t('sync_settings.conflict.snapshot') }}</dt>
            <dd class="break-words font-mono text-[13px] text-text">
              {{ snapshotLabel(state?.last_known_local_head) }}
            </dd>
          </div>
          <div class="flex flex-col gap-0.5">
            <dt class="text-xs text-text-dim">{{ $t('sync_settings.conflict.device') }}</dt>
            <dd class="text-[13px] text-text">
              {{ currentDeviceId || $t('sync_settings.conflict.current_device') }}
            </dd>
          </div>
          <div class="flex flex-col gap-0.5">
            <dt class="text-xs text-text-dim">{{ $t('sync_settings.conflict.last_checked') }}</dt>
            <dd class="font-mono text-[13px] text-text">{{ formatTime(state?.last_sync_at) }}</dd>
          </div>
        </dl>
      </section>

      <section class="rounded-md border border-border bg-surface-2 p-3">
        <h4 class="mb-2.5 text-sm font-medium text-text">
          {{ $t('sync_settings.conflict.cloud_title') }}
        </h4>
        <dl class="flex flex-col gap-2">
          <div class="flex flex-col gap-0.5">
            <dt class="text-xs text-text-dim">{{ $t('sync_settings.conflict.snapshot') }}</dt>
            <dd class="break-words font-mono text-[13px] text-text">
              {{ snapshotLabel(state?.last_known_remote_head) }}
            </dd>
          </div>
          <div class="flex flex-col gap-0.5">
            <dt class="text-xs text-text-dim">{{ $t('sync_settings.conflict.device') }}</dt>
            <dd class="text-[13px] text-text">{{ $t('sync_settings.conflict.cloud_device') }}</dd>
          </div>
          <div class="flex flex-col gap-0.5">
            <dt class="text-xs text-text-dim">{{ $t('sync_settings.conflict.last_checked') }}</dt>
            <dd class="font-mono text-[13px] text-text">{{ formatTime(state?.last_sync_at) }}</dd>
          </div>
        </dl>
      </section>
    </div>

    <template #footer>
      <KButton :disabled="resolving" @click="decideLater">
        {{ $t('sync_settings.conflict.decide_later') }}
      </KButton>
      <KButton :loading="resolving" @click="emit('resolve', 'accept_remote')">
        {{ $t('sync_settings.conflict.accept_remote') }}
      </KButton>
      <KButton variant="primary" :loading="resolving" @click="emit('resolve', 'keep_local')">
        {{ $t('sync_settings.conflict.keep_local') }}
      </KButton>
    </template>
  </KDialog>
</template>
