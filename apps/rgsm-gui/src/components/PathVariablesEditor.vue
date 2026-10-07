<script setup lang="ts">
import { computed, ref } from 'vue';
import { Ellipsis, Plus } from '@lucide/vue';
import type { Device, Game, GameDeviceBinding } from '../api/commands';
import { usePathVariableDraft } from '../composables/usePathVariableDraft';
import { KButton, KDialog, KInput, KMenu } from '../ui/kit';
import type { KMenuEntry } from '../ui/kit/KMenu.vue';
import { $t } from '../i18n';

const props = defineProps<{
  device: Device;
  binding?: GameDeviceBinding;
  paths: string[];
  games: Game[];
  gameKey?: string;
}>();
const emit = defineEmits<{ 'update:binding': [value: GameDeviceBinding] }>();
const draft = usePathVariableDraft()!;
const newName = ref('');
const adding = ref(false);
const defaultOpen = ref(false);
const defaultName = ref('');
const defaultValue = ref('');
const references = (paths: string[]) =>
  paths.flatMap((path) => [...path.matchAll(/<var:([A-Za-z0-9_-]+)>/g)].map((match) => match[1]!));
const names = computed(() =>
  [
    ...new Set([
      ...references(props.paths),
      ...Object.keys(props.binding?.pathVariables ?? {}),
      ...Object.keys(draft.edits.value),
    ]),
  ].sort()
);
const validNewName = computed(
  () => /^[A-Za-z0-9_-]+$/.test(newName.value.trim()) && !names.value.includes(newName.value.trim())
);
function defaults(name: string) {
  return draft.edits.value[name]?.value ?? props.device.path_variables?.[name];
}
function value(name: string) {
  return props.binding?.pathVariables?.[name] ?? defaults(name) ?? '';
}
function hasOverride(name: string) {
  return props.binding?.pathVariables?.[name] !== undefined;
}
function source(name: string) {
  if (hasOverride(name)) return $t('path_variables.game_value');
  if (draft.edits.value[name]) return $t('path_variables.default_pending');
  return defaults(name) !== undefined ? $t('path_variables.inherited') : $t('path_variables.unset');
}
function setGame(name: string, next: string | undefined) {
  const variables = { ...props.binding?.pathVariables };
  if (next === undefined) delete variables[name];
  else variables[name] = next;
  emit('update:binding', { ...props.binding, pathVariables: variables });
}
function entries(name: string): KMenuEntry[] {
  const items: KMenuEntry[] = [
    { type: 'item', key: 'default', label: $t('path_variables.edit_default') },
  ];
  if (hasOverride(name) && defaults(name) !== undefined)
    items.push({ type: 'item', key: 'inherit', label: $t('path_variables.reset') });
  if (draft.edits.value[name])
    items.push({ type: 'item', key: 'undo', label: $t('path_variables.undo_default') });
  if (draft.insert.value)
    items.push({ type: 'item', key: 'insert', label: $t('path_variables.insert') });
  if (hasOverride(name) && !references(props.paths).includes(name))
    items.push({ type: 'item', key: 'remove', label: $t('addgame.remove') });
  return items;
}
function action(name: string, key: string) {
  if (key === 'default') {
    defaultName.value = name;
    defaultValue.value = defaults(name) ?? value(name);
    defaultOpen.value = true;
  } else if (key === 'inherit' || key === 'remove') setGame(name, undefined);
  else if (key === 'insert') draft.insert.value?.(`<var:${name}>`);
  else if (key === 'undo') {
    const edits = { ...draft.edits.value };
    delete edits[name];
    draft.edits.value = edits;
  }
}
function applyDefault() {
  const name = defaultName.value;
  if (!defaultValue.value.trim() || /[<>\0]/.test(defaultValue.value)) return;
  const edits = { ...draft.edits.value };
  if (defaultValue.value === props.device.path_variables?.[name]) delete edits[name];
  else
    edits[name] = {
      value: defaultValue.value,
      previousValue: props.device.path_variables?.[name] ?? null,
    };
  draft.edits.value = edits;
  defaultOpen.value = false;
}
function affected(name: string) {
  return props.games
    .filter(
      (game) =>
        game.storage_key !== props.gameKey &&
        game.device_bindings?.[props.device.id]?.pathVariables?.[name] === undefined &&
        references([
          game.game_paths?.[props.device.id] ?? '',
          ...game.save_paths.map(
            (unit) =>
              game.device_bindings?.[props.device.id]?.pathOverrides?.[unit.id!]?.expression ??
              (unit.source.type === 'manifestPattern'
                ? unit.source.pattern
                : (unit.source.paths?.[props.device.id] ?? ''))
          ),
        ]).includes(name)
    )
    .map((game) => game.name);
}

function add() {
  if (!validNewName.value) return;
  const name = newName.value.trim();
  setGame(name, defaults(name) ?? '');
  newName.value = '';
  adding.value = false;
}
</script>

<template>
  <section class="min-w-0" data-testid="path-variables">
    <div class="flex h-8 items-center justify-between gap-3">
      <h3 class="text-sm font-medium">{{ $t('path_variables.title') }}</h3>
      <KButton size="sm" variant="ghost" @click="adding = true"
        ><Plus :size="14" aria-hidden="true" />{{ $t('path_variables.add') }}</KButton
      >
    </div>
    <p v-if="names.length" class="mt-1 text-xs text-text-dim">
      {{ $t('path_variables.edit_hint') }}
    </p>
    <div class="mt-4 flex flex-col gap-4">
      <div v-for="name in names" :key="name" class="min-w-0" :data-variable="name">
        <div class="mb-1 flex h-7 min-w-0 items-center gap-3">
          <label
            :for="`path-variable-${name}`"
            class="min-w-0 truncate font-mono text-xs"
            :title="name"
            >{{ name }}</label
          >
          <span class="ml-auto shrink-0 text-xs text-text-dim">{{ source(name) }}</span>
          <KMenu
            :entries="entries(name)"
            :aria-label="$t('path_variables.actions', { name })"
            @select="action(name, $event)"
          >
            <KButton
              size="sm"
              variant="ghost"
              class="p-0"
              :aria-label="$t('path_variables.actions', { name })"
              ><template #icon><Ellipsis :size="16" aria-hidden="true" /></template
            ></KButton>
          </KMenu>
        </div>
        <KInput
          :id="`path-variable-${name}`"
          class="w-full min-w-0"
          mono
          :aria-label="$t('path_variables.value', { name })"
          :model-value="value(name)"
          :title="value(name)"
          @update:model-value="setGame(name, String($event))"
        />
        <p v-if="!value(name).trim()" class="mt-1 text-xs text-danger">
          {{ $t('path_variables.missing', { name }) }}
        </p>
      </div>
    </div>
  </section>
  <KDialog v-model:open="adding" :title="$t('path_variables.add')" :width="420">
    <form @submit.prevent="add">
      <label for="new-path-variable" class="mb-2 block text-sm">{{
        $t('path_variables.name')
      }}</label>
      <KInput
        id="new-path-variable"
        v-model="newName"
        class="w-full"
        :placeholder="$t('path_variables.name_hint')"
        :aria-label="$t('path_variables.name')"
      />
    </form>
    <template #footer>
      <KButton variant="ghost" @click="adding = false">{{ $t('common.cancel') }}</KButton>
      <KButton variant="primary" :disabled="!validNewName" @click="add">{{
        $t('path_variables.add')
      }}</KButton>
    </template>
  </KDialog>
  <KDialog v-model:open="defaultOpen" :title="$t('path_variables.edit_default')" :width="440">
    <template #description>{{
      $t('path_variables.default_description', { device: device.name })
    }}</template>
    <label for="device-default-variable" class="mb-2 block font-mono text-xs">{{
      defaultName
    }}</label>
    <KInput
      id="device-default-variable"
      v-model="defaultValue"
      class="w-full"
      mono
      :aria-label="$t('path_variables.value', { name: defaultName })"
    />
    <p v-if="hasOverride(defaultName)" class="mt-3 text-xs text-text-dim">
      {{ $t('path_variables.override_kept') }}
    </p>
    <div class="mt-4 text-xs text-text-dim">
      <p>{{ $t('path_variables.affected', { count: affected(defaultName).length }) }}</p>
      <ul
        v-if="affected(defaultName).length"
        class="mt-2 max-h-28 space-y-1 overflow-y-auto text-text"
      >
        <li v-for="game in affected(defaultName)" :key="game" class="break-words">{{ game }}</li>
      </ul>
    </div>
    <template #footer>
      <KButton variant="ghost" @click="defaultOpen = false">{{ $t('common.cancel') }}</KButton>
      <KButton
        variant="primary"
        :disabled="!defaultValue.trim() || /[<>\0]/.test(defaultValue)"
        @click="applyDefault"
        >{{ $t('path_variables.use_change') }}</KButton
      >
    </template>
  </KDialog>
</template>
