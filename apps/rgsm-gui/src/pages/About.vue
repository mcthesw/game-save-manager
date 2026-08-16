<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { commands } from '~/api/commands';
import { $t } from '../i18n';
import { debug } from '../utils/logger';

const { config } = useConfig();

const gitHash = ref('');

onMounted(async () => {
  try {
    const info = await commands.getBuildInfo();
    gitHash.value = info.git_hash;
  } catch {
    gitHash.value = '';
  }
});

async function openUrl(url: string) {
  debug(`Opening URL: ${url}`);
  try {
    await commands.openUrl(url);
  } catch (reason) {
    notifyError($t('error.open_url_failed'));
    console.error(`Failed to open URL ${url}:`, reason);
  }
}

const contributors = [
  { name: 'Sworld', role: $t('about.project_initiator') },
  { name: 'Itsusinn逸新', role: $t('about.developer') },
  { name: 'AsterNighT', role: $t('about.developer') },
  { name: 'noneSycamore', role: $t('about.developer') },
  { name: '勺子', role: $t('about.ea_tester') },
  { name: 'Wali', role: $t('about.ea_tester') },
  { name: '土拨鼠', role: $t('about.ea_tester') },
  { name: '布莱泽', role: $t('about.ea_tester') },
  { name: 'Tostar.King', role: $t('about.developer') },
  { name: 'Summerraim', role: $t('about.developer') },
  { name: 'saschabuehrle', role: $t('about.developer') },
  { name: 'lucienlmy', role: $t('about.developer') },
  { name: 'PlanC', role: $t('about.developer') },
  { name: 'banzhe', role: $t('about.developer') },
  { name: 'Максим Горпиніч', role: $t('about.active_translator') },
  { name: 'தமிழ்நேரம்', role: $t('about.active_translator') },
];

const frontendDeps = [
  { name: 'Vue.js', url: 'https://vuejs.org/', license: 'MIT' },
  { name: 'Vue Router', url: 'https://router.vuejs.org/', license: 'MIT' },
  { name: 'Vite', url: 'https://vite.dev/', license: 'MIT' },
  { name: 'Reka UI', url: 'https://reka-ui.com/', license: 'MIT' },
  { name: 'Tailwind CSS', url: 'https://tailwindcss.com/', license: 'MIT' },
  { name: 'Lucide', url: 'https://lucide.dev/', license: 'ISC' },
  { name: 'VueUse', url: 'https://vueuse.org/', license: 'MIT' },
  { name: 'Vue Flow', url: 'https://vueflow.dev/', license: 'MIT' },
  { name: 'Vuedraggable', url: 'https://github.com/SortableJS/Vue.Draggable', license: 'MIT' },
  { name: 'Day.js', url: 'https://day.js.org/', license: 'MIT' },
  { name: 'UUID', url: 'https://github.com/uuidjs/uuid', license: 'MIT' },
];

const backendDeps = [
  { name: 'Tauri', url: 'https://tauri.app/', license: 'MIT / Apache-2.0' },
  {
    name: 'Ludusavi Manifest',
    url: 'https://github.com/mtkennerly/ludusavi-manifest',
    license: 'CC-BY-4.0',
  },
  { name: 'OpenDAL', url: 'https://opendal.apache.org/', license: 'Apache-2.0' },
  { name: 'Serde', url: 'https://serde.rs/', license: 'MIT / Apache-2.0' },
  { name: 'Tokio', url: 'https://tokio.rs/', license: 'MIT' },
  { name: 'Chrono', url: 'https://github.com/chronotope/chrono', license: 'MIT' },
  { name: 'Reqwest', url: 'https://github.com/seanmonstar/reqwest', license: 'MIT' },
  { name: 'zip-rs', url: 'https://github.com/zip-rs/zip', license: 'MIT' },
  { name: 'Rodio', url: 'https://github.com/RustAudio/rodio', license: 'MIT' },
];

const headerLinks = [
  { label: 'Gitee', url: 'https://gitee.com/sworldS/game-save-manager' },
  { label: 'GitHub', url: 'https://github.com/mcthesw/game-save-manager' },
  { label: $t('about.official_website'), url: 'https://game.sworld.club/' },
  { label: $t('about.help'), url: 'https://help.sworld.club/' },
  {
    label: $t('about.help_translate'),
    url: 'https://github.com/mcthesw/game-save-manager/blob/main/CONTRIBUTING.md',
  },
];
</script>

<template>
  <div class="h-full overflow-y-auto">
    <div class="mx-auto flex max-w-[720px] flex-col gap-8 px-6 py-8">
      <header class="flex flex-col items-center pt-4 text-center">
        <img src="/orange.png" alt="App Logo" class="mb-4 h-20 w-20" />
        <h1 class="text-2xl font-semibold text-text">{{ $t('home.name') }}</h1>
        <div class="mt-2 rounded-full bg-surface-2 px-2.5 py-0.5 font-mono text-xs text-text-dim">
          v{{ config?.version }}<span v-if="gitHash" class="opacity-70"> ({{ gitHash }})</span>
        </div>
        <p class="mt-3 max-w-md text-sm leading-relaxed text-text-dim">
          {{ $t('about.content_1') }}
        </p>
        <div class="mt-4 flex flex-wrap items-center justify-center gap-x-3 gap-y-1">
          <template v-for="(link, index) in headerLinks" :key="link.url">
            <span v-if="index > 0" class="h-3.5 w-px bg-border" aria-hidden="true" />
            <button
              type="button"
              class="cursor-pointer border-none bg-transparent p-0 text-sm text-accent transition-colors hover:brightness-110"
              @click="openUrl(link.url)"
            >
              {{ link.label }}
            </button>
          </template>
        </div>
      </header>

      <section>
        <h2 class="mb-2 text-sm font-semibold text-text">{{ $t('about.support_me') }}</h2>
        <div class="flex flex-col gap-1.5 text-sm leading-relaxed text-text-dim">
          <p>{{ $t('about.support_me_content_1') }}</p>
          <p>{{ $t('about.support_me_content_2') }}</p>
        </div>
      </section>

      <section>
        <h2 class="mb-3 border-b border-border pb-2 text-sm font-semibold text-text">
          {{ $t('about.thank_you_list') }}
        </h2>
        <div class="grid grid-cols-2 gap-x-6 gap-y-2 sm:grid-cols-3">
          <div v-for="c in contributors" :key="c.name" class="flex items-baseline gap-2">
            <span class="truncate text-sm text-text">{{ c.name }}</span>
            <span class="shrink-0 text-xs text-text-dim">{{ c.role }}</span>
          </div>
        </div>
      </section>

      <section>
        <h2 class="mb-3 border-b border-border pb-2 text-sm font-semibold text-text">
          {{ $t('about.open_source_acknowledgments') }}
        </h2>
        <div class="grid grid-cols-1 gap-6 sm:grid-cols-2">
          <div>
            <h3 class="mb-2 text-xs font-medium text-text-dim">Frontend</h3>
            <div class="flex flex-col gap-1.5">
              <div v-for="lib in frontendDeps" :key="lib.name" class="flex items-baseline gap-2">
                <button
                  type="button"
                  class="cursor-pointer border-none bg-transparent p-0 text-left text-sm text-accent transition-colors hover:brightness-110"
                  @click="openUrl(lib.url)"
                >
                  {{ lib.name }}
                </button>
                <span class="text-xs text-text-dim">{{ lib.license }}</span>
              </div>
            </div>
          </div>
          <div>
            <h3 class="mb-2 text-xs font-medium text-text-dim">Backend</h3>
            <div class="flex flex-col gap-1.5">
              <div v-for="lib in backendDeps" :key="lib.name" class="flex items-baseline gap-2">
                <button
                  type="button"
                  class="cursor-pointer border-none bg-transparent p-0 text-left text-sm text-accent transition-colors hover:brightness-110"
                  @click="openUrl(lib.url)"
                >
                  {{ lib.name }}
                </button>
                <span class="text-xs text-text-dim">{{ lib.license }}</span>
              </div>
            </div>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>
