import { computed, ref } from 'vue';
import { $t } from '../i18n';

interface LoadingEntry {
  message: string;
  detail?: string;
}

const messageStack = ref<LoadingEntry[]>([]);

function startLoading(message?: string, detail?: string) {
  messageStack.value.push({
    message: message ?? $t('common.operation_in_progress'),
    detail,
  });
}

function stopLoading() {
  if (messageStack.value.length > 0) {
    messageStack.value.pop();
  }
}

async function withLoading<T>(
  operation: () => Promise<T>,
  message?: string,
  detail?: string
): Promise<T> {
  startLoading(message, detail);
  try {
    return await operation();
  } finally {
    stopLoading();
  }
}

const isLoading = computed(() => messageStack.value.length > 0);
const loadingMessage = computed(() => {
  const top = messageStack.value[messageStack.value.length - 1];
  return top?.message ?? $t('common.operation_in_progress');
});
const loadingDetail = computed(() => {
  const top = messageStack.value[messageStack.value.length - 1];
  return top?.detail;
});

export function useGlobalLoading() {
  return {
    isLoading,
    loadingMessage,
    loadingDetail,
    startLoading,
    stopLoading,
    withLoading,
  };
}
