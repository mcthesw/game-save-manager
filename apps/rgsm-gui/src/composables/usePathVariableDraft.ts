import { computed, inject, provide, ref, shallowRef, type InjectionKey, type Ref } from 'vue';
import type { Device, DeviceVariableEdit, GameDeviceBinding } from '../api/commands';

export type VariableEdits = Record<string, DeviceVariableEdit>;
const key: InjectionKey<ReturnType<typeof createDraft>> = Symbol('pathVariableDraft');
function createDraft(
  device: Ref<Device | null | undefined>,
  binding: Ref<GameDeviceBinding | undefined>
) {
  const edits = ref<VariableEdits>({});
  const insert = shallowRef<((token: string) => void) | null>(null);
  const names = computed(() =>
    [
      ...new Set([
        ...Object.keys(device.value?.path_variables ?? {}),
        ...Object.keys(binding.value?.pathVariables ?? {}),
        ...Object.keys(edits.value),
      ]),
    ].sort()
  );
  return {
    edits,
    names,
    insert,
    reset: () => {
      edits.value = {};
    },
  };
}
export function providePathVariableDraft(
  device: Ref<Device | null | undefined>,
  binding: Ref<GameDeviceBinding | undefined>
) {
  const draft = createDraft(device, binding);
  provide(key, draft);
  return draft;
}
export function usePathVariableDraft() {
  return inject(key, null);
}
