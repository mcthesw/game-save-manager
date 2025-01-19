<script setup lang="ts">
import 'element-plus/theme-chalk/dark/css-vars.css' // 引入暗黑主题样式
const { config, refreshConfig } = useConfig();
useDark();

import { events } from "./bindings";
import { useNotification } from "./composables/useNotification";
import { useConfig } from "./composables/useConfig";
import { $t } from "./i18n";

const { showInfo, showWarning, showError } = useNotification();

try {
  await refreshConfig()
  await navigateTo(config.value!.settings.home_page)
} catch (e) {
  showError({ message: $t("home.wrong_homepage") })
  navigateTo("/")
}

events.ipcNotification.listen((event) => {
  let ev = event.payload;
  switch (ev.level) {
    case "info":
      showInfo({ message: ev.msg, title: ev.title });
      break;
    case "warning":
      showWarning({ message: ev.msg, title: ev.title });
      break;
    case "error":
      showError({ message: ev.msg, title: ev.title });
      break;
  }
});
</script>

<template>
  <div>
    <NuxtLayout>
      <NuxtPage />
    </NuxtLayout>
  </div>
</template>

<style>
body {
  margin: 0px !important;
}

.page-enter-active,
.page-leave-active {
  transition: all 0.2s ease-out;
}

.page-enter-from,
.page-leave-to {
  opacity: 0.4;
  filter: blur(0.2rem);
}
</style>