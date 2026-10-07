<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref } from 'vue';
import { Archive, RefreshCw } from '@lucide/vue';
import {
  commands,
  type LocalUpgradeAction,
  type LocalUpgradeView,
  type UpgradeRetry,
} from '../api/commands';
import { notifyError, notifySuccess } from '../composables/useActivityCenter';
import { useFeedback } from '../composables/useFeedback';
import { $t } from '../i18n';
import { KButton } from '../ui/kit';
import LocalUpgradePendingItem from './LocalUpgradePendingItem.vue';

const view = ref<LocalUpgradeView>();
const loading = ref(true);
const busy = ref(false);
const running = ref(false);
const pauseRequested = ref(false);
const failed = ref(false);
const feedback = useFeedback();
const visible = computed(
  () =>
    loading.value ||
    failed.value ||
    (!!view.value &&
      (view.value.remaining > 0 || view.value.originalCount > 0 || view.value.pending.length > 0))
);

function size(bytes: number) {
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const index = Math.min(
    Math.floor(Math.log(Math.max(bytes, 1)) / Math.log(1024)),
    units.length - 1
  );
  return `${new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(bytes / 1024 ** index)} ${units[index]}`;
}

async function request(action: LocalUpgradeAction) {
  const result = await commands.localArchiveUpgrade(action);
  if (result.status === 'error') throw new Error(result.error);
  view.value = result.data;
}

async function load() {
  loading.value = true;
  failed.value = false;
  try {
    await request({ action: 'preview' });
  } catch {
    failed.value = true;
  } finally {
    loading.value = false;
  }
}

async function run() {
  if (busy.value || running.value) return;
  running.value = true;
  pauseRequested.value = false;
  try {
    if (!view.value?.started) await request({ action: 'start' });
    while (!pauseRequested.value && (view.value?.remaining ?? 0) > 0) {
      await request({ action: 'step' });
    }
  } catch (error) {
    notifyError($t('local_upgrade.failed'), String(error));
  } finally {
    running.value = false;
  }
}

async function retry(requestBody: UpgradeRetry) {
  busy.value = true;
  let queued = false;
  try {
    await request({ action: 'retry', request: requestBody });
    queued = true;
  } catch (error) {
    notifyError($t('local_upgrade.failed'), String(error));
  } finally {
    busy.value = false;
  }
  if (queued) await run();
}

async function cleanup() {
  try {
    await feedback.confirm($t('local_upgrade.cleanup_confirm'), $t('local_upgrade.cleanup'), {
      confirmButtonText: $t('local_upgrade.cleanup'),
      type: 'warning',
    });
  } catch {
    return;
  }
  busy.value = true;
  try {
    await request({ action: 'cleanup' });
    if (!view.value?.originalCount) notifySuccess($t('local_upgrade.cleaned'));
  } catch (error) {
    notifyError($t('local_upgrade.failed'), String(error));
  } finally {
    busy.value = false;
  }
}

onMounted(load);
onBeforeUnmount(() => {
  pauseRequested.value = true;
});
</script>

<template>
  <section v-if="visible" data-testid="local-upgrade" class="min-w-0">
    <div class="mb-3 flex items-center gap-2 border-b border-border pb-2">
      <Archive :size="15" class="text-text-dim" aria-hidden="true" />
      <h2 class="text-sm font-semibold text-text">{{ $t('local_upgrade.title') }}</h2>
      <KButton
        v-if="!loading && !running"
        variant="ghost"
        size="sm"
        class="ml-auto"
        :aria-label="$t('local_upgrade.refresh')"
        :disabled="busy"
        @click="load"
      >
        <RefreshCw :size="14" />
      </KButton>
    </div>
    <p v-if="loading" class="text-sm text-text-dim">{{ $t('local_upgrade.loading') }}</p>
    <p v-else-if="failed" class="text-sm text-danger">{{ $t('local_upgrade.load_failed') }}</p>
    <template v-else-if="view">
      <div class="flex flex-wrap items-start justify-between gap-4">
        <div class="min-w-0 flex-1 basis-64">
          <p class="text-sm font-medium text-text" role="status">
            {{
              $t(view.started ? 'local_upgrade.progress' : 'local_upgrade.found', {
                completed: view.completed,
                total: view.total,
              })
            }}
          </p>
          <p class="mt-1 text-xs leading-relaxed text-text-dim">
            {{ $t('local_upgrade.purpose') }}
          </p>
          <p v-if="view.remaining" class="mt-1 text-xs text-text-dim">
            {{ $t('local_upgrade.space', { size: size(view.estimatedExtraBytes) }) }}
            <span v-if="view.unknownSizes">{{ $t('local_upgrade.space_unknown') }}</span>
          </p>
        </div>
        <KButton v-if="running" :disabled="pauseRequested" @click="pauseRequested = true">
          {{ $t(pauseRequested ? 'local_upgrade.pausing' : 'local_upgrade.pause') }}
        </KButton>
        <KButton v-else-if="view.remaining" variant="primary" :disabled="busy" @click="run">
          {{ $t(view.started ? 'local_upgrade.resume' : 'local_upgrade.start') }}
        </KButton>
      </div>
      <progress
        v-if="running || (view.remaining && view.started)"
        :value="view.completed"
        :max="view.total"
        :aria-label="$t('local_upgrade.title')"
        class="mt-3 h-1 w-full accent-accent"
      />
      <p v-if="running" class="mt-2 text-xs text-text-dim">{{ $t('local_upgrade.pause_hint') }}</p>
      <div v-if="view.pending.length" class="mt-5">
        <h3 class="mb-1 text-sm font-medium text-text">
          {{ $t('local_upgrade.pending', { count: view.pending.length }) }}
        </h3>
        <LocalUpgradePendingItem
          v-for="item in view.pending"
          :key="item.id"
          :item="item"
          :disabled="busy || running"
          @retry="retry"
        />
      </div>
      <div v-if="view.originalCount" class="mt-4 flex flex-wrap items-center justify-between gap-2">
        <p class="text-xs text-text-dim">
          {{
            $t('local_upgrade.originals', {
              count: view.originalCount,
              size: size(view.originalBytes),
            })
          }}
        </p>
        <KButton variant="ghost" size="sm" :disabled="busy || running" @click="cleanup">
          {{ $t('local_upgrade.cleanup') }}
        </KButton>
      </div>
    </template>
  </section>
</template>
