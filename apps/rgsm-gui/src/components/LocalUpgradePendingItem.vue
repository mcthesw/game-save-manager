<script setup lang="ts">
import { computed, ref } from 'vue';
import { FolderOpen } from '@lucide/vue';
import { commands, type UpgradePendingItem, type UpgradeRetry } from '../api/commands';
import { notifyError } from '../composables/useActivityCenter';
import { $t } from '../i18n';
import { KButton, KInput, KSelect } from '../ui/kit';

const props = defineProps<{ item: UpgradePendingItem; disabled: boolean }>();
const emit = defineEmits<{ retry: [request: UpgradeRetry] }>();
const replacement = ref('');
const unit = ref<string | number>();
const canChooseCopy = computed(
  () => !['catalogUnreadable', 'originalChanged', 'historyChanged'].includes(props.item.issue.kind)
);
const needsAssociation = computed(() => props.item.issue.kind === 'associationRequired');
const filename = computed(() => props.item.sourcePath.replaceAll('\\', '/').split('/').at(-1));

async function chooseCopy() {
  try {
    const result = await commands.chooseSaveFile();
    if (result.status === 'ok' && result.data) replacement.value = result.data;
    else if (result.status === 'error') notifyError(result.error);
  } catch (error) {
    notifyError(String(error));
  }
}
function retry() {
  emit('retry', {
    itemId: props.item.id,
    replacementPath: replacement.value.trim() || null,
    archiveEntry: props.item.issue.archiveEntry,
    saveUnitId: unit.value == null ? null : Number(unit.value),
  });
}
</script>

<template>
  <details class="group border-b border-border py-3 last:border-0">
    <summary class="cursor-pointer text-sm text-text">
      <span class="font-medium">{{ item.gameName }}</span>
      <span class="ml-2 break-all text-xs text-text-dim">{{ filename }}</span>
    </summary>
    <div class="mt-3 min-w-0 space-y-3 pl-4">
      <p class="text-sm leading-relaxed text-text">
        {{ $t(`local_upgrade.issues.${item.issue.kind}`) }}
      </p>
      <p class="break-all text-xs text-text-dim">{{ item.sourcePath }}</p>
      <div v-if="canChooseCopy" class="min-w-0">
        <label class="mb-1.5 block text-xs text-text-dim" :for="`copy-${item.id}`">{{
          $t('local_upgrade.copy')
        }}</label>
        <div class="flex min-w-0 gap-1">
          <KInput
            :id="`copy-${item.id}`"
            v-model="replacement"
            class="min-w-0 flex-1"
            :disabled="disabled"
            :placeholder="$t('local_upgrade.copy_placeholder')"
          />
          <KButton
            variant="ghost"
            :disabled="disabled"
            :aria-label="$t('local_upgrade.choose_copy')"
            @click="chooseCopy"
          >
            <FolderOpen :size="16" />
          </KButton>
        </div>
      </div>
      <div v-if="needsAssociation" class="min-w-0">
        <p class="mb-1.5 break-all text-xs text-text-dim">
          {{ $t('local_upgrade.association', { entry: item.issue.archiveEntry ?? '' }) }}
        </p>
        <KSelect
          v-model="unit"
          :options="
            item.saveUnits.map((u, index) => ({
              value: u.id,
              label: `${index + 1}. ${u.path}`,
            }))
          "
          :disabled="disabled"
          :placeholder="$t('local_upgrade.choose_unit')"
          :aria-label="$t('local_upgrade.choose_unit')"
          class="w-full min-w-0 [&>span:first-child]:truncate"
        />
      </div>
      <KButton
        v-if="item.issue.kind !== 'originalChanged'"
        size="sm"
        :disabled="disabled || (needsAssociation && unit == null)"
        @click="retry"
      >
        {{ $t('local_upgrade.retry') }}
      </KButton>
    </div>
  </details>
</template>
