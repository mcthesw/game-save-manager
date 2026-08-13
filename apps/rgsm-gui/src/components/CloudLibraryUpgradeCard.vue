<script setup lang="ts">
defineProps<{
  kicker: string;
  title: string;
  hint: string;
  action: string;
}>();

const emit = defineEmits<{
  (event: 'action'): void;
}>();
</script>

<template>
  <section class="upgrade-card">
    <div class="upgrade-copy">
      <p class="upgrade-kicker">{{ kicker }}</p>
      <p class="upgrade-title">{{ title }}</p>
      <p class="upgrade-hint">{{ hint }}</p>
    </div>
    <ElButton type="primary" size="large" @click="emit('action')">
      {{ action }}
    </ElButton>
  </section>
</template>

<style scoped>
.upgrade-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 24px;
  margin-bottom: 20px;
  padding: 18px 20px;
  border: 1px solid color-mix(in oklab, var(--el-color-warning) 38%, var(--el-border-color-lighter));
  border-radius: 10px;
  background: color-mix(in oklab, var(--el-color-warning-light-9) 78%, var(--el-bg-color));
  box-shadow: inset 3px 0 0 var(--el-color-warning);
}

.upgrade-copy {
  min-width: 0;
}

.upgrade-kicker,
.upgrade-title,
.upgrade-hint {
  margin: 0;
  line-height: 1.45;
}

.upgrade-kicker {
  margin-bottom: 4px;
  color: var(--el-color-warning-dark-2);
  font-size: 0.78rem;
  font-weight: 700;
  letter-spacing: 0.04em;
}

.upgrade-title {
  color: var(--el-text-color-primary);
  font-size: 1.05rem;
  font-weight: 650;
}

.upgrade-hint {
  margin-top: 6px;
  color: var(--el-text-color-regular);
  font-size: 0.86rem;
}

.upgrade-card :deep(.el-button) {
  flex-shrink: 0;
  min-width: 148px;
}

@media (max-width: 640px) {
  .upgrade-card {
    align-items: flex-start;
    flex-direction: column;
  }
}
</style>
