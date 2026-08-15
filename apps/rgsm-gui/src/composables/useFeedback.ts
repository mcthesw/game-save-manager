import { ref, type VNode } from 'vue';

/**
 * Centralized confirm/prompt/alert façade on the kit dialog.
 * Call sites keep the ElMessageBox-era call shapes: options use
 * `type`/`confirmButtonText`/`inputPattern`/etc., prompt resolves `{ value }`,
 * and cancellation rejects with 'cancel'.
 */
export type FeedbackTone = 'info' | 'warning' | 'error';

export interface FeedbackOptions {
  confirmButtonText?: string;
  cancelButtonText?: string;
  type?: FeedbackTone;
  closeOnClickModal?: boolean;
  closeOnPressEscape?: boolean;
}

export interface PromptOptions extends FeedbackOptions {
  inputValue?: string;
  inputPlaceholder?: string;
  inputPattern?: RegExp;
  inputErrorMessage?: string;
}

/** Resolution shape of prompt(); mirrors what call sites destructure today. */
export interface PromptResult {
  value: string;
}

export interface FeedbackRequest {
  kind: 'alert' | 'confirm' | 'prompt';
  title: string;
  message: string | VNode;
  tone: FeedbackTone;
  confirmText?: string;
  cancelText?: string;
  inputValue?: string;
  inputPlaceholder?: string;
  inputPattern?: RegExp;
  inputErrorMessage?: string;
  dismissable: boolean;
  resolve: (result: PromptResult | undefined) => void;
  reject: (reason: 'cancel') => void;
}

const feedbackQueue = ref<FeedbackRequest[]>([]);

export function useFeedbackQueue() {
  return { feedbackQueue };
}

function enqueue(
  kind: FeedbackRequest['kind'],
  message: string | VNode,
  title: string,
  options?: PromptOptions
): Promise<PromptResult | undefined> {
  const { promise, resolve, reject } = Promise.withResolvers<PromptResult | undefined>();
  feedbackQueue.value.push({
    kind,
    title,
    message,
    tone: options?.type ?? 'info',
    confirmText: options?.confirmButtonText,
    cancelText: options?.cancelButtonText,
    inputValue: options?.inputValue,
    inputPlaceholder: options?.inputPlaceholder,
    inputPattern: options?.inputPattern,
    inputErrorMessage: options?.inputErrorMessage,
    dismissable: options?.closeOnClickModal !== false && options?.closeOnPressEscape !== false,
    resolve,
    reject,
  });
  return promise;
}

/** Called by the feedback host when the user confirms or dismisses a request. */
export function settleFeedback(
  request: FeedbackRequest,
  confirmed: boolean,
  result?: PromptResult
) {
  const index = feedbackQueue.value.indexOf(request);
  if (index !== -1) {
    feedbackQueue.value.splice(index, 1);
  }
  if (confirmed) {
    request.resolve(result);
  } else {
    request.reject('cancel');
  }
}

export function useFeedback() {
  return {
    alert: (message: string | VNode, title: string, options?: FeedbackOptions) =>
      enqueue('alert', message, title, options).then(() => undefined),

    confirm: (message: string | VNode, title: string, options?: FeedbackOptions) =>
      enqueue('confirm', message, title, options).then(() => undefined),

    prompt: (message: string, title: string, options?: PromptOptions) =>
      enqueue('prompt', message, title, options) as Promise<PromptResult>,
  };
}
