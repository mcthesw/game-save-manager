<script lang="ts" setup>
import { computed, ref } from 'vue';
import { X } from '@lucide/vue';
import { $t } from '../i18n';
import { KButton } from '../ui/kit';

// Initialize hotkey_out with default values
const hotkey_out = defineModel<{
  backup: string[];
  apply: string[];
}>({
  default: {
    backup: ['', '', ''],
    apply: ['', '', ''],
  },
});

/**
 * Slot layout kept for the backend: [modifier1, modifier2, key], '' = unset.
 * The capture UI records real key events and serializes into these slots.
 */
interface Combo {
  mods: string[];
  key: string;
}

function parseCombo(slots: string[] | undefined): Combo {
  const [m1 = '', m2 = '', key = ''] = slots ?? [];
  return { mods: [m1, m2].filter(Boolean), key };
}

function toSlots(combo: Combo): string[] {
  return [combo.mods[0] ?? '', combo.mods[1] ?? '', combo.key];
}

type HotkeySlot = 'backup' | 'apply';

const capturing = ref<HotkeySlot | null>(null);

function comboText(combo: Combo): string {
  return [...combo.mods, combo.key].filter(Boolean).join(' + ');
}

function normalizeKey(event: KeyboardEvent): string | null {
  const key = event.key;
  if (['Control', 'Shift', 'Alt', 'Meta'].includes(key)) return null;
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(key)) return key;
  if (key.length === 1) return key.toUpperCase();
  return null;
}

function onCaptureKey(slot: HotkeySlot, event: KeyboardEvent) {
  event.preventDefault();
  event.stopPropagation();
  if (event.key === 'Escape') {
    capturing.value = null;
    return;
  }
  const key = normalizeKey(event);
  if (!key) return; // 纯修饰键按下:继续等待完整组合
  const mods = [
    event.ctrlKey ? 'CONTROL' : '',
    event.shiftKey ? 'SHIFT' : '',
    event.altKey ? 'ALT' : '',
  ].filter(Boolean);
  hotkey_out.value = { ...hotkey_out.value, [slot]: toSlots({ mods, key }) };
  capturing.value = null;
}

function clearCombo(slot: HotkeySlot) {
  hotkey_out.value = { ...hotkey_out.value, [slot]: ['', '', ''] };
}

const rows = computed(() => [
  {
    slot: 'backup' as const,
    label: $t('settings.hotkey.quick_backup'),
    combo: parseCombo(hotkey_out.value.backup),
  },
  {
    slot: 'apply' as const,
    label: $t('settings.hotkey.quick_apply'),
    combo: parseCombo(hotkey_out.value.apply),
  },
]);
</script>

<template>
  <div class="flex flex-col gap-3">
    <p class="text-xs leading-relaxed text-text-dim">{{ $t('settings.hotkey.hint') }}</p>
    <div v-for="row in rows" :key="row.slot" class="flex items-center gap-2">
      <span class="w-20 shrink-0 text-xs text-text-dim">{{ row.label }}</span>
      <button
        type="button"
        class="box-border inline-flex h-9 min-w-56 cursor-pointer items-center rounded-sm border px-3 text-left text-sm transition-colors duration-150 focus-visible:outline-2 focus-visible:outline-accent"
        :class="
          capturing === row.slot
            ? 'border-accent bg-accent-soft text-text'
            : 'border-border bg-surface text-text hover:border-border-strong'
        "
        :aria-label="row.label"
        @click="capturing = row.slot"
        @keydown="onCaptureKey(row.slot, $event)"
        @blur="capturing === row.slot && (capturing = null)"
      >
        <span v-if="capturing === row.slot" class="text-text-dim">
          {{ $t('settings.hotkey.capturing') }}
        </span>
        <span v-else-if="comboText(row.combo)" class="font-mono">{{ comboText(row.combo) }}</span>
        <span v-else class="text-text-dim">{{ $t('settings.hotkey.not_set') }}</span>
      </button>
      <KButton
        v-if="comboText(row.combo)"
        variant="ghost"
        size="sm"
        :aria-label="$t('settings.hotkey.clear')"
        @click="clearCombo(row.slot)"
      >
        <template #icon><X :size="14" aria-hidden="true" /></template>
      </KButton>
    </div>
  </div>
</template>
