<script setup lang="ts">
import { KButton } from '../ui/kit';
import { useAppUpdates } from '../composables/useAppUpdates';
const props = withDefaults(defineProps<{ noticeOnly?: boolean }>(), { noticeOnly: false });
const {
  update,
  checking,
  busy,
  ready,
  status,
  state,
  progressPercent,
  showBanner,
  checkForUpdates,
  applyUpdate,
  openRelease,
  manualDownload,
  cancelInstall,
  dismiss,
} = useAppUpdates();
</script>

<template>
  <section
    v-if="!props.noticeOnly || showBanner"
    :class="
      props.noticeOnly
        ? 'flex flex-wrap items-center justify-between gap-3 border-b border-border pb-3'
        : 'flex flex-wrap items-center justify-between gap-x-4 gap-y-1.5 py-0.5'
    "
  >
    <div class="min-w-0" role="status">
      <span class="block text-sm text-text">{{ $t('updates.title') }}</span>
      <span v-if="status === 'check-error'" class="block text-xs text-text-dim">{{
        $t('updates.check_failed')
      }}</span>
      <span v-else-if="status === 'install-error'" class="block text-xs text-text-dim">{{
        $t('updates.update_failed')
      }}</span>
      <span v-else-if="state?.stage === 'waiting'" class="block text-xs text-text-dim">{{
        $t('updates.waiting')
      }}</span>
      <span v-else-if="state?.stage === 'installing'" class="block text-xs text-text-dim">{{
        $t('updates.installing')
      }}</span>
      <span v-else-if="ready" class="block text-xs text-text-dim">{{
        $t('updates.ready', { version: update?.latestVersion })
      }}</span>
      <span v-else-if="state?.stage === 'downloading'" class="block text-xs text-text-dim">
        {{ $t('updates.downloading')
        }}<span v-if="progressPercent !== null"> {{ progressPercent }}%</span>
      </span>
      <span v-else-if="update?.available" class="block text-xs text-text-dim">{{
        $t('updates.available', { version: update.latestVersion })
      }}</span>
      <span v-else-if="status === 'current'" class="block text-xs text-text-dim">{{
        $t('updates.current')
      }}</span>
    </div>
    <div class="flex flex-wrap items-center gap-2">
      <KButton
        v-if="!props.noticeOnly && !ready"
        size="sm"
        :loading="checking"
        :disabled="busy"
        @click="checkForUpdates()"
        >{{ $t('updates.check') }}</KButton
      >
      <KButton
        v-if="update?.available"
        :variant="props.noticeOnly ? 'default' : 'primary'"
        size="sm"
        :loading="busy"
        :disabled="checking"
        @click="applyUpdate"
      >
        {{ ready ? $t('updates.install') : $t('updates.download') }}
      </KButton>
      <KButton v-if="state?.stage === 'waiting'" variant="ghost" size="sm" @click="cancelInstall">{{
        $t('updates.cancel_wait')
      }}</KButton>
      <KButton v-if="update?.available && !busy" variant="ghost" size="sm" @click="openRelease">{{
        $t('updates.release_notes')
      }}</KButton>
      <KButton
        v-if="status === 'install-error' && update?.action === 'install'"
        variant="ghost"
        size="sm"
        @click="manualDownload"
        >{{ $t('updates.manual_download') }}</KButton
      >
      <KButton v-if="props.noticeOnly && !busy" variant="ghost" size="sm" @click="dismiss">{{
        $t('updates.later')
      }}</KButton>
    </div>
  </section>
</template>
