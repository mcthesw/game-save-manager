<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import {
  commands,
  type CloudArchiveGameView,
  type InitialCatchUpPolicy,
  type SyncMode,
} from '../api/commands';
import { notifyError, notifySuccess } from '../composables/useActivityCenter';
import { $t } from '../i18n';
import { CheckCircle2 } from '@lucide/vue';
import { KAlert, KButton, KDialog, KInput, KSwitch } from '../ui/kit';
import {
  cloudArchiveCatchUpPreview as catchUpPreview,
  formatCloudArchiveBytes as formatBytes,
} from '../utils/cloudArchivePresentation';

const props = defineProps<{
  game: CloudArchiveGameView | null;
  mode: SyncMode;
}>();

const emit = defineEmits<{
  (event: 'update:game', value: CloudArchiveGameView | null): void;
  (event: 'updated'): void;
}>();

const changingMode = ref(false);
const catchUpPolicy = ref<InitialCatchUpPolicy>('keep_remote');
const liveSaveProcessName = ref('');
const liveSaveSnapshotOnExit = ref(false);
const isLive = computed(() => props.mode === 'multi_device_sync');

const visible = computed({
  get: () => props.game !== null,
  set: (value: boolean) => {
    if (!value) emit('update:game', null);
  },
});

watch(
  () => props.game,
  (game) => {
    catchUpPolicy.value = 'keep_remote';
    liveSaveProcessName.value = game?.live_save_process_name ?? '';
    liveSaveSnapshotOnExit.value = game?.live_save_snapshot_on_exit ?? false;
  }
);

const catchUpOptions = computed(() => [
  {
    value: 'keep_remote' as const,
    title: $t('sync_settings.archives.keep_remote'),
    desc: $t('sync_settings.archives.keep_remote_description'),
  },
  {
    value: 'download_existing' as const,
    title: $t('sync_settings.archives.download_existing'),
    desc: $t('sync_settings.archives.download_existing_description', {
      count: catchUpPreview(props.game).count,
      size: formatBytes(catchUpPreview(props.game).size),
    }),
  },
]);

async function confirm() {
  const game = props.game;
  if (!game) return;
  changingMode.value = true;
  try {
    const result = await commands.setGameSyncMode(
      game.game_id,
      props.mode,
      catchUpPolicy.value,
      isLive.value && liveSaveProcessName.value.trim()
        ? {
            process_name: liveSaveProcessName.value.trim(),
            snapshot_on_exit: liveSaveSnapshotOnExit.value,
          }
        : null
    );
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.mode_change_failed'), result.error);
      emit('update:game', null);
      emit('updated');
      return;
    }
    emit('update:game', null);
    notifySuccess(
      result.data.downloaded > 0
        ? $t('sync_settings.archives.mode_enabled_downloaded', {
            count: result.data.downloaded,
          })
        : $t('sync_settings.archives.mode_changed')
    );
    emit('updated');
  } finally {
    changingMode.value = false;
  }
}
</script>

<template>
  <KDialog
    v-model:open="visible"
    :title="
      isLive
        ? $t('sync_settings.archives.enable_live_save_sync')
        : $t('sync_settings.archives.enable_snapshot_sync')
    "
    :width="480"
    :dismissable="!changingMode"
  >
    <KAlert tone="warning" class="mb-3">
      {{
        isLive
          ? $t('sync_settings.archives.live_save_sync_risk')
          : $t('sync_settings.archives.snapshot_sync_risk')
      }}
    </KAlert>
    <ul class="mb-4 list-disc pl-5 text-sm leading-relaxed text-text-dim">
      <li>
        {{
          isLive
            ? $t('sync_settings.archives.live_save_sync_description')
            : $t('sync_settings.archives.snapshot_sync_description')
        }}
      </li>
      <li>
        {{
          isLive
            ? $t('sync_settings.archives.live_save_sync_description_2')
            : $t('sync_settings.archives.snapshot_sync_description_2')
        }}
      </li>
      <li v-if="isLive">
        {{ $t('sync_settings.archives.live_save_sync_description_3') }}
      </li>
    </ul>

    <div v-if="isLive" class="mb-4 flex flex-col gap-3">
      <div>
        <div class="mb-1 block text-xs text-text-dim">
          {{ $t('sync_settings.archives.live_save_process') }}
        </div>
        <KInput
          v-model="liveSaveProcessName"
          class="w-full"
          mono
          :placeholder="$t('sync_settings.archives.live_save_process_placeholder')"
          :aria-label="$t('sync_settings.archives.live_save_process')"
        />
      </div>
      <div class="flex items-center justify-between gap-4">
        <div class="min-w-0">
          <div class="text-sm text-text">{{ $t('sync_settings.archives.snapshot_on_exit') }}</div>
          <div class="text-xs text-text-dim">
            {{ $t('sync_settings.archives.snapshot_on_exit_description') }}
          </div>
        </div>
        <KSwitch v-model="liveSaveSnapshotOnExit" />
      </div>
    </div>

    <p class="mb-2 text-sm font-medium text-text">
      {{ $t('sync_settings.archives.catch_up_title') }}
    </p>
    <div class="flex flex-col gap-2">
      <button
        v-for="option in catchUpOptions"
        :key="option.value"
        type="button"
        class="flex cursor-pointer items-start gap-2.5 rounded-md border px-3 py-2.5 text-left transition-colors focus-visible:outline-2 focus-visible:outline-accent"
        :class="
          catchUpPolicy === option.value
            ? 'border-accent bg-accent-soft'
            : 'border-border bg-surface hover:border-border-strong'
        "
        :aria-pressed="catchUpPolicy === option.value"
        @click="catchUpPolicy = option.value"
      >
        <CheckCircle2
          v-if="catchUpPolicy === option.value"
          :size="15"
          class="mt-0.5 shrink-0 text-accent"
          aria-hidden="true"
        />
        <span
          v-else
          class="mt-0.5 inline-block h-[15px] w-[15px] shrink-0 rounded-full border border-border-strong"
          aria-hidden="true"
        />
        <span class="min-w-0">
          <span class="block text-sm font-medium text-text">{{ option.title }}</span>
          <span class="mt-0.5 block text-xs leading-relaxed text-text-dim">{{ option.desc }}</span>
        </span>
      </button>
    </div>

    <template #footer>
      <KButton :disabled="changingMode" @click="visible = false">
        {{ $t('sync_settings.cancel') }}
      </KButton>
      <KButton
        variant="primary"
        :disabled="isLive && liveSaveSnapshotOnExit && !liveSaveProcessName.trim()"
        :loading="changingMode"
        @click="confirm"
      >
        {{ $t('sync_settings.archives.enable') }}
      </KButton>
    </template>
  </KDialog>
</template>
