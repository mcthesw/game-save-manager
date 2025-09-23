import { computed, ref } from 'vue'
import { $t } from '../i18n'

const messageStack = ref<string[]>([])

function startLoading(message?: string) {
  messageStack.value.push(message ?? $t('common.operation_in_progress'))
}

function stopLoading() {
  if (messageStack.value.length > 0) {
    messageStack.value.pop()
  }
}

async function withLoading<T>(operation: () => Promise<T>, message?: string): Promise<T> {
  startLoading(message)
  try {
    return await operation()
  } finally {
    stopLoading()
  }
}

const isLoading = computed(() => messageStack.value.length > 0)
const loadingMessage = computed(() => {
  if (messageStack.value.length === 0) {
    return $t('common.operation_in_progress')
  }
  return messageStack.value[messageStack.value.length - 1]
})

export function useGlobalLoading() {
  return {
    isLoading,
    loadingMessage,
    startLoading,
    stopLoading,
    withLoading,
  }
}
