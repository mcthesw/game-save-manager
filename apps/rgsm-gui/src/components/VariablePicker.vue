<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { KButton, KDialog, KInput } from '../ui/kit';
import { $t } from '../i18n';
const open = defineModel<boolean>('open', { default: false });
const props = defineProps<{
  variables: { name: string; value: string; labelKey: string }[];
  values?: Record<string, string>;
  canCreate: boolean;
}>();
const emit = defineEmits<{ select: [token: string]; create: [] }>();
const search = ref('');
const label = (v: (typeof props.variables)[number]) =>
  v.labelKey ? $t(`path_variable.${v.labelKey}`) : v.name.replace(/^var:/, '');
const filtered = computed(() =>
  props.variables.filter((v) =>
    `${label(v)} ${v.name}`.toLowerCase().includes(search.value.toLowerCase())
  )
);
watch(open, () => {
  search.value = '';
});
function select(token: string) {
  open.value = false;
  emit('select', token);
}
function create() {
  open.value = false;
  emit('create');
}
</script>
<template>
  <KDialog v-model:open="open" :title="$t('path_variable.insert_variable')" :width="440">
    <KInput
      v-model="search"
      class="w-full"
      :placeholder="$t('path_variables.search')"
      :aria-label="$t('path_variables.search')"
    />
    <div class="mt-3 max-h-72 overflow-y-auto">
      <button
        v-for="v in filtered"
        :key="v.name"
        type="button"
        class="flex w-full cursor-pointer flex-col gap-1 rounded border-0 bg-transparent px-3 py-2 text-left text-text hover:bg-surface-2 focus-visible:outline-2 focus-visible:outline-accent"
        @click="select(v.value)"
      >
        <span class="text-sm">{{ label(v) }}</span>
        <span
          class="max-w-full truncate font-mono text-xs text-text-dim"
          :title="values?.[v.name.replace(/^var:/, '')] || v.value"
          >{{ values?.[v.name.replace(/^var:/, '')] || v.value }}</span
        >
      </button>
      <p v-if="!filtered.length" class="py-4 text-sm text-text-dim">
        {{ $t('path_variables.no_results') }}
      </p>
    </div>
    <template #footer
      ><KButton v-if="canCreate" @click="create">{{ $t('path_variables.create_insert') }}</KButton
      ><KButton variant="ghost" @click="open = false">{{ $t('manage.cancel') }}</KButton></template
    >
  </KDialog>
</template>
