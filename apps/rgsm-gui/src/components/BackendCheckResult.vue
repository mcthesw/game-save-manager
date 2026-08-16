<script setup lang="ts">
import { computed, ref } from 'vue';
import { CheckCircle2, Copy, LoaderCircle, TriangleAlert, XCircle } from '@lucide/vue';
import { $t } from '../i18n';
import type {
  CloudBackendCheckItem,
  CloudBackendCheckReport,
  CloudBackendCheckStep,
} from '../api/commands';
import { KButton, KPopover } from '../ui/kit';

type CheckOutcome = CloudBackendCheckReport['outcome'];
type CheckItemStatus = CloudBackendCheckItem['status'];
type UiOutcome = CheckOutcome | 'unknown';
type UiCheckItem = {
  step: CloudBackendCheckStep;
  status: CheckItemStatus | 'pending';
  message: string | null;
};

const props = withDefaults(
  defineProps<{
    report: CloudBackendCheckReport | null;
    checking?: boolean;
    disabled?: boolean;
    error?: string | null;
  }>(),
  {
    checking: false,
    disabled: false,
    error: null,
  }
);

const emit = defineEmits<{
  (event: 'test'): void;
}>();

const copiedStep = ref<string | null>(null);

const STEPS: CloudBackendCheckStep[] = [
  'prepare_backend',
  'list_files',
  'write_file',
  'read_file',
  'verify_content',
  'delete_file',
];

const STEP_LABELS: Record<CloudBackendCheckStep, string> = {
  prepare_backend: 'sync_settings.check_result.steps.prepare_backend',
  list_files: 'sync_settings.check_result.steps.list_files',
  write_file: 'sync_settings.check_result.steps.write_file',
  read_file: 'sync_settings.check_result.steps.read_file',
  verify_content: 'sync_settings.check_result.steps.verify_content',
  delete_file: 'sync_settings.check_result.steps.delete_file',
};

const OUTCOME_META: Record<UiOutcome, { titleKey: string; descKey: string }> = {
  available: {
    titleKey: 'sync_settings.check_result.available_title',
    descKey: 'sync_settings.check_result.available_desc',
  },
  degraded: {
    titleKey: 'sync_settings.check_result.degraded_title',
    descKey: 'sync_settings.check_result.degraded_desc',
  },
  unavailable: {
    titleKey: 'sync_settings.check_result.unavailable_title',
    descKey: 'sync_settings.check_result.unavailable_desc',
  },
  unknown: {
    titleKey: 'sync_settings.check_result.unknown_title',
    descKey: 'sync_settings.check_result.unknown_desc',
  },
};

const currentOutcome = computed<UiOutcome>(() => {
  if (props.error) return 'unavailable';
  return props.report?.outcome ?? 'unknown';
});

const indicatorColor = computed(() => {
  if (props.checking) return 'var(--accent)';
  switch (currentOutcome.value) {
    case 'available':
      return 'var(--success)';
    case 'degraded':
      return 'var(--warning)';
    case 'unavailable':
      return 'var(--danger)';
    default:
      return 'var(--text-dim)';
  }
});

const checkItems = computed<UiCheckItem[]>(() =>
  STEPS.map((step) => {
    const item = props.report?.items.find((candidate) => candidate.step === step);
    return item
      ? { ...item, message: item.message ?? null }
      : { step, status: 'pending', message: null };
  })
);

const titleKey = computed(() =>
  props.checking
    ? 'sync_settings.check_result.checking_title'
    : OUTCOME_META[currentOutcome.value].titleKey
);

const descriptionKey = computed(() => {
  // 可用态不赘述:绿点+全过步骤已足够;只有异常态才给说明
  if (props.checking || currentOutcome.value === 'unknown' || currentOutcome.value === 'available')
    return null;
  return OUTCOME_META[currentOutcome.value].descKey;
});

function categorizeError(msg: string | null): string {
  if (!msg) return '';
  if (/timeout|timed out/i.test(msg))
    return $t('sync_settings.check_result.error_category.timeout');
  if (/401|403|unauthorized|forbidden/i.test(msg))
    return $t('sync_settings.check_result.error_category.auth');
  if (/404|not found/i.test(msg)) return $t('sync_settings.check_result.error_category.not_found');
  if (/network|connect|dns|resolve/i.test(msg))
    return $t('sync_settings.check_result.error_category.network');
  if (/virtual.host/i.test(msg)) return $t('sync_settings.check_result.error_category.addressing');
  return $t('sync_settings.check_result.error_category.general');
}

function truncate(str: string, max: number): string {
  return str.length > max ? str.slice(0, max) + '...' : str;
}

async function copyError(message: string, key: string) {
  await navigator.clipboard.writeText(message);
  copiedStep.value = key;
  setTimeout(() => {
    copiedStep.value = null;
  }, 2000);
}
</script>

<template>
  <section class="flex flex-col" aria-live="polite">
    <div class="flex items-center gap-2">
      <span
        class="h-2 w-2 shrink-0 rounded-full"
        :style="{ background: indicatorColor }"
        aria-hidden="true"
      />
      <span class="text-sm font-semibold leading-snug text-text">{{ $t(titleKey) }}</span>
      <LoaderCircle v-if="checking" :size="13" class="shrink-0 animate-spin text-accent" />
    </div>
    <p v-if="descriptionKey" class="mt-1 text-xs leading-snug text-text-dim">
      {{ $t(descriptionKey) }}
    </p>

    <div v-if="report && !checking" class="mt-2 flex flex-col gap-1.5">
      <div v-for="item in checkItems" :key="item.step" class="flex items-center gap-2">
        <span
          v-if="item.status === 'pending'"
          class="h-3.5 w-3.5 shrink-0 rounded-full border-[1.5px] border-border"
          aria-hidden="true"
        />
        <CheckCircle2
          v-else-if="item.status === 'passed'"
          :size="14"
          class="shrink-0 text-success"
          aria-hidden="true"
        />
        <XCircle
          v-else-if="item.status === 'failed'"
          :size="14"
          class="shrink-0 text-danger"
          aria-hidden="true"
        />
        <TriangleAlert v-else :size="14" class="shrink-0 text-warning" aria-hidden="true" />
        <span
          class="min-w-0 flex-1 text-xs"
          :class="item.status === 'pending' ? 'text-text-dim/70' : 'text-text'"
        >
          {{ $t(STEP_LABELS[item.step]) }}
        </span>
        <KPopover v-if="item.message" side="bottom" align="start" :width="360">
          <KButton variant="ghost" size="sm">
            {{ $t('sync_settings.check_result.view_error') }}
          </KButton>
          <template #content>
            <p class="mb-2 text-[13px] font-semibold text-danger">
              {{ categorizeError(item.message) }}
            </p>
            <pre
              class="mb-2.5 max-h-52 overflow-auto rounded-sm border border-border bg-surface-2 p-2 text-xs leading-relaxed whitespace-pre-wrap break-all text-text-dim"
              >{{ truncate(item.message, 500) }}</pre
            >
            <KButton size="sm" @click="copyError(item.message ?? '', item.step)">
              <template #icon><Copy :size="12" aria-hidden="true" /></template>
              {{
                copiedStep === item.step
                  ? $t('sync_settings.check_result.copied')
                  : $t('sync_settings.check_result.copy_error')
              }}
            </KButton>
          </template>
        </KPopover>
      </div>
    </div>

    <div
      v-if="error"
      class="mt-3 flex items-center justify-between gap-2 rounded-md bg-danger-soft px-2.5 py-2 text-xs text-danger"
    >
      <span>{{ categorizeError(error) }}</span>
      <KButton variant="ghost" size="sm" class="text-danger" @click="copyError(error, 'command')">
        {{
          copiedStep === 'command'
            ? $t('sync_settings.check_result.copied')
            : $t('sync_settings.check_result.copy_error')
        }}
      </KButton>
    </div>

    <div class="mt-3">
      <KButton
        variant="primary"
        size="sm"
        :loading="checking"
        :disabled="disabled || checking"
        @click="emit('test')"
      >
        {{
          report || error
            ? $t('sync_settings.check_result.retest')
            : $t('sync_settings.test_button')
        }}
      </KButton>
    </div>
  </section>
</template>
