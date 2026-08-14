import { defineComponent, h, ref } from 'vue';
import { $t } from '../i18n';
import type { Settings } from '../api/commands';
import { ElCheckbox } from '../ui/elementPlus/checkbox';
import { useConfig } from './useConfig';
import { useFeedback } from './useFeedback';

type ApplyConfirmationTarget = 'latest' | 'snapshot';
type ApplyConfirmationSetting = Extract<
  keyof Settings,
  'confirm_before_apply_latest' | 'confirm_before_apply_snapshot'
>;

const CONFIRMATION_SETTING_BY_TARGET: Record<ApplyConfirmationTarget, ApplyConfirmationSetting> = {
  latest: 'confirm_before_apply_latest',
  snapshot: 'confirm_before_apply_snapshot',
};

export function useApplyConfirmation() {
  const { config, saveConfig } = useConfig();
  const feedback = useFeedback();

  async function confirmBeforeApply(target: ApplyConfirmationTarget): Promise<boolean> {
    const setting = CONFIRMATION_SETTING_BY_TARGET[target];
    if (config.value.settings[setting] === false) {
      return true;
    }

    const skipNextTime = ref(false);
    const ApplyConfirmationMessage = defineComponent({
      name: 'ApplyConfirmationMessage',
      setup() {
        return () =>
          h('div', { class: 'apply-confirmation-message' }, [
            h('p', $t('manage.confirm_overwrite_prompt')),
            h(
              ElCheckbox,
              {
                modelValue: skipNextTime.value,
                'onUpdate:modelValue': (value: boolean | string | number) => {
                  skipNextTime.value = value === true;
                },
              },
              () => $t('manage.do_not_ask_again')
            ),
          ]);
      },
    });

    try {
      await feedback.confirm(h(ApplyConfirmationMessage), $t('manage.warning'), {
        confirmButtonText: $t('manage.confirm'),
        cancelButtonText: $t('manage.cancel'),
        type: 'warning',
      });
    } catch {
      return false;
    }

    if (skipNextTime.value) {
      const previousValue = config.value.settings[setting];
      config.value.settings[setting] = false;

      const saved = await saveConfig();
      if (!saved) {
        config.value.settings[setting] = previousValue;
      }
    }

    return true;
  }

  async function confirmAndRun(
    target: ApplyConfirmationTarget,
    action: () => Promise<void> | void
  ): Promise<boolean> {
    const confirmed = await confirmBeforeApply(target);
    if (!confirmed) {
      return false;
    }

    await action();
    return true;
  }

  return {
    confirmBeforeApply,
    confirmAndRun,
  };
}
