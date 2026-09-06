<script setup lang="ts">
import { computed, onUnmounted, ref, shallowRef, watch } from 'vue';
import { useWindowFocus, useDocumentVisibility } from '@vueuse/core';
import { commands, events } from '../api/commands';
import type { PendingProgress, ProgressNotice } from '../api/generated/types.gen';
import { useCloudLibrary } from '../composables/useCloudLibrary';
import { notifyError } from '../composables/useActivityCenter';
import { $t } from '../i18n';
import { overlayDepth } from '../ui/overlayDepth';
import { KButton, KDialog } from '../ui/kit';
import V2ConflictReviewDialog from './V2ConflictReviewDialog.vue';

const { library, refresh } = useCloudLibrary();
const focused = useWindowFocus();
const visibility = useDocumentVisibility();
const pending = shallowRef<PendingProgress | null>(null);
const selected = shallowRef<ProgressNotice | null>(null);
const dismissed = ref(new Set<string>());
const open = ref(false);
const busy = ref(false);
const notices = computed(() =>
  pending.value?.library_id === library.value?.library_id
    ? (pending.value?.notices.filter((notice) => !dismissed.value.has(notice.id)) ?? [])
    : []
);

const subscription = events.remoteProgressPending.listen(({ payload }) => {
  pending.value = payload;
  const current = new Set(payload.notices.map((notice) => notice.id));
  dismissed.value = new Set([...dismissed.value].filter((id) => current.has(id)));
});
onUnmounted(() => {
  void subscription.then((stop) => stop());
});

async function defer(items: ProgressNotice[]) {
  if (!items.length) return;
  const ids = items.map((item) => item.id);
  ids.forEach((id) => dismissed.value.add(id));
  busy.value = true;
  try {
    const result = await commands.deferProgressNotices(ids);
    if (result.status === 'error') throw new Error(result.error);
  } catch (error) {
    ids.forEach((id) => dismissed.value.delete(id));
    notifyError($t('sync_settings.archives.progress.resolve_failed'), String(error));
  } finally {
    busy.value = false;
  }
}

function later(value: boolean) {
  open.value = value;
  if (!value) void defer(notices.value);
}

function compare(notice: ProgressNotice) {
  selected.value = notice;
  open.value = false;
}

function closeComparison(value: boolean) {
  if (value || !selected.value) return;
  const notice = selected.value;
  selected.value = null;
  void defer([notice]);
}

watch(
  () => library.value?.library_id,
  () => {
    open.value = false;
    selected.value = null;
  }
);
watch(
  [notices, focused, visibility, overlayDepth, selected, busy],
  () => {
    if (!notices.value.length) open.value = false;
    else if (
      !open.value &&
      !selected.value &&
      !busy.value &&
      focused.value &&
      visibility.value === 'visible' &&
      overlayDepth.value === 0
    )
      open.value = true;
  },
  { flush: 'post', immediate: true }
);
</script>

<template>
  <KDialog
    :open="open"
    :title="$t('sync_settings.archives.progress.pending_title')"
    :width="480"
    @update:open="later"
  >
    <template #description>
      {{ $t('sync_settings.archives.progress.pending_description') }}
    </template>
    <ul class="max-h-[calc(100dvh-15rem)] overflow-y-auto">
      <li
        v-for="notice in notices"
        :key="notice.id"
        class="flex items-center justify-between gap-3 py-2"
      >
        <span class="min-w-0 truncate text-sm">{{ notice.game_name }}</span>
        <KButton size="sm" @click="compare(notice)">{{
          $t('sync_settings.archives.progress.pending_compare')
        }}</KButton>
      </li>
    </ul>
    <template #footer>
      <KButton @click="later(false)">{{
        $t('sync_settings.archives.progress.pending_later')
      }}</KButton>
    </template>
  </KDialog>
  <V2ConflictReviewDialog
    v-if="selected"
    :model-value="true"
    :game-id="selected.game_id"
    :game-name="selected.game_name"
    @update:model-value="closeComparison"
    @resolved="refresh(true)"
  />
</template>
