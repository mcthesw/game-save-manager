import { ElMessageBox } from 'element-plus';
import { LAYER } from '../ui/layers';

// A small, centralized façade for user feedback patterns.
// - Keep business code focused on intent (confirm/prompt) instead of UI library details.
// - Provide consistent defaults (e.g. z-index), without changing copy/text.

type ConfirmOptions = Parameters<typeof ElMessageBox.confirm>[2];
type PromptOptions = Parameters<typeof ElMessageBox.prompt>[2];

function withDefaults(options: ConfirmOptions): ConfirmOptions {
  if (!options) {
    return { zIndex: LAYER.messageBox } as unknown as ConfirmOptions;
  }
  // Avoid overriding user-provided values; only fill in missing fields.
  if ('zIndex' in options) return options;
  return { ...options, zIndex: LAYER.messageBox } as unknown as ConfirmOptions;
}

function withPromptDefaults(options: PromptOptions): PromptOptions {
  if (!options) {
    return { zIndex: LAYER.messageBox } as unknown as PromptOptions;
  }
  if ('zIndex' in options) return options;
  return { ...options, zIndex: LAYER.messageBox } as unknown as PromptOptions;
}

export function useFeedback() {
  return {
    confirm: (message: string, title: string, options?: ConfirmOptions) =>
      ElMessageBox.confirm(message, title, withDefaults(options)),

    prompt: (message: string, title: string, options?: PromptOptions) =>
      ElMessageBox.prompt(message, title, withPromptDefaults(options)),
  };
}
