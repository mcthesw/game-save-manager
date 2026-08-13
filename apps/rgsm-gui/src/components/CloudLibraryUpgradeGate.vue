<script setup lang="ts">
import { LAYER } from '~/ui/layers';

defineProps<{
  blocked: boolean;
  title: string;
  action: string;
}>();

const emit = defineEmits<{
  (event: 'upgrade'): void;
}>();
</script>

<template>
  <div class="upgrade-gate-host" :class="{ 'is-blocked': blocked }">
    <slot />
    <div v-if="blocked" class="upgrade-gate">
      <div class="upgrade-gate-card">
        <p>{{ title }}</p>
        <ElButton type="primary" @click="emit('upgrade')">
          {{ action }}
        </ElButton>
      </div>
    </div>
  </div>
</template>

<style scoped>
.upgrade-gate-host {
  position: relative;
}

.upgrade-gate-host.is-blocked {
  min-height: 280px;
}

.upgrade-gate {
  position: absolute;
  inset: 0;
  z-index: v-bind('LAYER.base + 1');
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 28px 20px;
  overflow: hidden;
  border-radius: 8px;
  background:
    linear-gradient(
      180deg,
      var(--el-bg-color) 0%,
      transparent 26%,
      transparent 78%,
      var(--el-bg-color) 100%
    ),
    linear-gradient(
      90deg,
      var(--el-bg-color) 0%,
      transparent 14%,
      transparent 88%,
      var(--el-bg-color) 100%
    ),
    color-mix(in oklab, var(--el-bg-color) 38%, transparent);
}

.upgrade-gate::before {
  content: '';
  position: absolute;
  inset: 0;
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
  -webkit-mask-image:
    linear-gradient(180deg, transparent 0%, #000 22%, #000 80%, transparent 100%),
    linear-gradient(90deg, transparent 0%, #000 12%, #000 90%, transparent 100%);
  -webkit-mask-composite: source-in;
  mask-image:
    linear-gradient(180deg, transparent 0%, #000 22%, #000 80%, transparent 100%),
    linear-gradient(90deg, transparent 0%, #000 12%, #000 90%, transparent 100%);
  mask-composite: intersect;
  pointer-events: none;
}
.upgrade-gate-card {
  position: relative;
  z-index: 1;
  display: flex;
  max-width: 360px;
  flex-direction: column;
  align-items: center;
  gap: 14px;
  padding: 8px 12px;
  text-align: center;
}

.upgrade-gate-card p {
  margin: 0;
  color: var(--el-text-color-primary);
  font-size: 0.98rem;
  font-weight: 600;
  line-height: 1.5;
}
</style>
