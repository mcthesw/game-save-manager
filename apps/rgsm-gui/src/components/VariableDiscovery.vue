<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { commands, type GameDraft } from '../api/commands';
import type { VariableEdits } from '../composables/usePathVariableDraft';
import { KButton, KDialog, KInput } from '../ui/kit';
import { $t } from '../i18n';
const props = defineProps<{
  name: string;
  game: GameDraft;
  paths: string[];
  deviceVariables: VariableEdits;
}>();
const emit = defineEmits<{ close: []; select: [value: string] }>();
const open = ref(true);
const busy = ref(true);
const error = ref('');
const selected = ref('');
const manual = ref('');
const result = ref<{
  candidates: { value: string; paths: string[] }[];
  missingVariables: string[];
  needsRoot: boolean;
  incomplete: boolean;
}>();
onMounted(async () => {
  try {
    const response = await commands.discoverPathVariable({
      game: props.game,
      paths: props.paths,
      name: props.name,
      deviceVariables: props.deviceVariables,
    });
    if (response.status === 'error') error.value = response.error;
    else result.value = response.data;
  } catch {
    error.value = $t('path_variables.find_failed');
  } finally {
    busy.value = false;
  }
});
async function browse() {
  const response = await commands.chooseSaveDir();
  if (response.status === 'ok' && response.data) {
    manual.value = response.data;
    selected.value = '';
  }
}
function useValue() {
  const value = manual.value || selected.value;
  if (value.trim() && !/[<>\0]/.test(value)) emit('select', value);
}
</script>
<template>
  <KDialog
    v-model:open="open"
    :title="$t('path_variables.find_title', { name })"
    :width="560"
    @update:open="!$event && emit('close')"
  >
    <template #description>{{ $t('path_variables.find_hint') }}</template>
    <p v-if="busy" class="text-sm text-text-dim" role="status">
      {{ $t('path_variables.finding') }}
    </p>
    <p v-else-if="error" class="text-sm text-danger" role="alert">{{ error }}</p>
    <template v-else-if="result">
      <p v-if="result.missingVariables.length" class="text-sm text-text-dim">
        {{ $t('path_variables.find_dependencies', { names: result.missingVariables.join(', ') }) }}
      </p>
      <p v-else-if="result.needsRoot" class="text-sm text-text-dim">
        {{ $t('path_variables.find_root') }}
      </p>
      <p v-else-if="!result.candidates.length" class="text-sm text-text-dim">
        {{ $t('path_variables.find_empty') }}
      </p>
      <div
        v-if="result.candidates.length"
        class="max-h-64 space-y-2 overflow-y-auto"
        role="radiogroup"
        :aria-label="$t('path_variables.find_candidates')"
      >
        <label
          v-for="candidate in result.candidates"
          :key="candidate.value"
          class="flex cursor-pointer items-start gap-3 rounded px-2 py-2 hover:bg-surface-2"
        >
          <input
            v-model="selected"
            type="radio"
            name="variable-candidate"
            :value="candidate.value"
            class="mt-1"
            @change="manual = ''"
          />
          <span class="min-w-0"
            ><span class="block break-all font-mono text-sm">{{ candidate.value }}</span
            ><span
              v-for="path in candidate.paths.slice(0, 1)"
              :key="path"
              class="mt-1 block break-all text-xs text-text-dim"
              >{{ path }}</span
            ></span
          >
        </label>
      </div>
      <p v-if="result.incomplete" class="mt-3 text-sm text-warning">
        {{ $t('path_variables.find_partial') }}
      </p>
    </template>
    <label for="variable-manual-value" class="mb-2 mt-4 block text-sm">{{
      $t('path_variables.find_manual')
    }}</label>
    <div class="flex min-w-0 items-center gap-2">
      <KInput
        id="variable-manual-value"
        v-model="manual"
        class="min-w-0 flex-1"
        :aria-label="$t('path_variables.find_manual')"
        @update:model-value="selected = ''"
      />
      <KButton @click="browse">{{ $t('path_variables.choose_folder') }}</KButton>
    </div>
    <template #footer>
      <KButton variant="ghost" @click="emit('close')">{{ $t('manage.cancel') }}</KButton>
      <KButton
        variant="primary"
        :disabled="!(manual || selected).trim() || /[<>\0]/.test(manual || selected)"
        @click="useValue"
        >{{ $t('path_variables.use_game_value') }}</KButton
      >
    </template>
  </KDialog>
</template>
