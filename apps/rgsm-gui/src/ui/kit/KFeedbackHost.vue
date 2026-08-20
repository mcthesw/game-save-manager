<script setup lang="ts">
import { computed, ref, watch, type VNode } from 'vue';
import { $t } from '../../i18n';
import { settleFeedback, useFeedbackQueue } from '../../composables/useFeedback';
import { LAYER } from '../layers';
import KButton from './KButton.vue';
import KDialog from './KDialog.vue';
import KInput from './KInput.vue';
/** Singleton host rendering queued useFeedback requests. Mounted once in App.vue. */
const { feedbackQueue } = useFeedbackQueue();

const current = computed(() => feedbackQueue.value[0]);
const inputText = ref('');
const inputError = ref('');

watch(current, (request) => {
  inputText.value = request?.inputValue ?? '';
  inputError.value = '';
});

const dialogOpen = computed({
  get: () => current.value !== undefined,
  set: (open: boolean) => {
    if (!open && current.value) {
      settleFeedback(current.value, false);
    }
  },
});

function onConfirm() {
  const request = current.value;
  if (!request) return;
  if (
    request.kind === 'prompt' &&
    request.inputPattern &&
    !request.inputPattern.test(inputText.value)
  ) {
    inputError.value = request.inputErrorMessage ?? '';
    return;
  }
  settleFeedback(request, true, request.kind === 'prompt' ? { value: inputText.value } : undefined);
}

function onCancel() {
  if (current.value) {
    settleFeedback(current.value, false);
  }
}

/** VNode messages (e.g. ApplyConfirmationMessage) render via a functional wrapper. */
function asRenderable(message: string | VNode) {
  return () => message;
}
</script>

<template>
  <KDialog
    v-model:open="dialogOpen"
    :title="current?.title ?? ''"
    :width="440"
    :dismissable="current?.dismissable ?? true"
    :layer="LAYER.messageBox"
  >
    <template v-if="current">
      <div class="text-sm leading-relaxed text-text">
        <component :is="asRenderable(current.message)" v-if="typeof current.message !== 'string'" />
        <p v-else class="whitespace-pre-wrap">{{ current.message }}</p>
      </div>
      <div v-if="current.kind === 'prompt'" class="mt-3">
        <KInput
          v-model="inputText"
          class="w-full"
          :placeholder="current.inputPlaceholder"
          :aria-label="current.title"
          @keyup.enter="onConfirm"
        />
        <p v-if="inputError" class="mt-1.5 text-xs text-danger">{{ inputError }}</p>
      </div>
    </template>
    <template v-if="current" #footer>
      <KButton v-if="current.kind !== 'alert'" @click="onCancel">
        {{ current.cancelText ?? $t('common.cancel') }}
      </KButton>
      <KButton :variant="current.tone === 'error' ? 'danger' : 'primary'" @click="onConfirm">
        {{ current.confirmText ?? $t('common.confirm') }}
      </KButton>
    </template>
  </KDialog>
</template>
