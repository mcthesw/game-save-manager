<script lang="ts" setup>
import { commands } from '~/bindings';
import { $t } from '../i18n';
import { debug } from '@tauri-apps/plugin-log';

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
  } catch (error) {
    notifyError($t('error.open_url_failed'));
    console.error(`Failed to open URL ${url}:`, error);
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
  { name: 'Element Plus', url: 'https://element-plus.org/', license: 'MIT' },
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
  { name: 'Chrono', url: 'https://github.com/chronotope/chrono', license: 'MIT / Apache-2.0' },
  { name: 'Reqwest', url: 'https://github.com/seanmonstar/reqwest', license: 'MIT / Apache-2.0' },
  { name: 'zip-rs', url: 'https://github.com/zip-rs/zip', license: 'MIT' },
  { name: 'Rodio', url: 'https://github.com/RustAudio/rodio', license: 'MIT / Apache-2.0' },
];
</script>

<template>
  <div class="about-page">
    <div class="content-wrapper">
      <el-scrollbar>
        <div class="main-content">
          <header class="app-header">
            <img src="/orange.png" alt="App Logo" class="app-logo" />
            <h1 class="app-title">{{ $t('home.name') }}</h1>
            <div v-if="config && config.version" class="version-badge">
              v{{ config.version }}
              <span v-if="gitHash" class="git-hash">({{ gitHash }})</span>
            </div>
            <p class="app-description">{{ $t('about.content_1') }}</p>

            <div class="header-links">
              <el-link @click="openUrl('https://gitee.com/sworldS/game-save-manager')"
                >Gitee</el-link
              >
              <el-divider direction="vertical" />
              <el-link @click="openUrl('https://github.com/mcthesw/game-save-manager')"
                >Github</el-link
              >
              <el-divider direction="vertical" />
              <el-link @click="openUrl('https://game.sworld.club/')">
                {{ $t('about.official_website') }}
              </el-link>
              <el-divider direction="vertical" />
              <el-link @click="openUrl('https://help.sworld.club/')">{{
                $t('about.help')
              }}</el-link>
            </div>
          </header>

          <section class="content-section">
            <h2 class="section-title">{{ $t('about.support_me') }}</h2>
            <div class="support-content">
              <p>{{ $t('about.support_me_content_1') }}</p>
              <p>{{ $t('about.support_me_content_2') }}</p>
            </div>
          </section>

          <el-divider />

          <section class="content-section">
            <h2 class="section-title">{{ $t('about.thank_you_list') }}</h2>
            <div class="contributors-list">
              <div v-for="c in contributors" :key="c.name" class="contributor-item">
                <span class="contributor-name">{{ c.name }}</span>
                <span class="contributor-role">{{ c.role }}</span>
              </div>
            </div>
          </section>

          <el-divider />

          <section class="content-section">
            <h2 class="section-title">{{ $t('about.open_source_acknowledgments') }}</h2>
            <div class="deps-container">
              <div class="deps-column">
                <h3 class="deps-subtitle">Frontend</h3>
                <div class="deps-list">
                  <div v-for="lib in frontendDeps" :key="lib.name" class="dep-row">
                    <el-link
                      type="primary"
                      underline="never"
                      class="dep-name"
                      @click="openUrl(lib.url)"
                    >
                      {{ lib.name }}
                    </el-link>
                    <span class="dep-license">{{ lib.license }}</span>
                  </div>
                </div>
              </div>
              <div class="deps-column">
                <h3 class="deps-subtitle">Backend</h3>
                <div class="deps-list">
                  <div v-for="lib in backendDeps" :key="lib.name" class="dep-row">
                    <el-link
                      type="primary"
                      underline="never"
                      class="dep-name"
                      @click="openUrl(lib.url)"
                    >
                      {{ lib.name }}
                    </el-link>
                    <span class="dep-license">{{ lib.license }}</span>
                  </div>
                </div>
              </div>
            </div>
          </section>
        </div>
      </el-scrollbar>
    </div>
  </div>
</template>

<style scoped>
.about-page {
  height: 100%;
  display: flex;
  flex-direction: column;
  background-color: var(--el-bg-color);
  overflow: hidden; /* Ensure no double scrollbars */
}

.content-wrapper {
  flex: 1;
  min-height: 0; /* Critical for flex child scrolling */
}

.main-content {
  padding: 3rem 15% 4rem;
  max-width: 1000px;
  margin: 0 auto;
}

.app-header {
  text-align: center;
  margin-bottom: 3rem;
}

.app-logo {
  width: 80px;
  height: 80px;
  margin-bottom: 1rem;
}

.app-title {
  font-size: 1.75rem;
  font-weight: 600;
  margin: 0 0 0.5rem;
  color: var(--el-text-color-primary);
}

.version-badge {
  display: inline-block;
  font-size: 0.9rem;
  color: var(--el-color-info);
  background-color: var(--el-fill-color-light);
  padding: 2px 8px;
  border-radius: 10px;
  margin-bottom: 1rem;
  font-family: var(--el-font-family-monospace);
}

.git-hash {
  font-size: 0.8rem;
  opacity: 0.7;
}

.app-description {
  font-size: 1rem;
  color: var(--el-text-color-secondary);
  line-height: 1.5;
}

.header-links {
  margin-top: 1.5rem;
  display: flex;
  justify-content: center;
  align-items: center;
  gap: 0.5rem;
}

.content-section {
  margin-bottom: 2rem;
}

.section-title {
  font-size: 1.25rem;
  font-weight: 600;
  margin-bottom: 1.5rem;
  color: var(--el-text-color-primary);
}

/* Support Section */
.support-content p {
  line-height: 1.6;
  color: var(--el-text-color-regular);
  margin-bottom: 0.5rem;
}

/* Contributors List */
.contributors-list {
  display: grid;
  grid-template-columns: 1fr;
  gap: 0.5rem;
}

@media (min-width: 768px) {
  .contributors-list {
    grid-template-columns: 1fr 1fr;
    gap: 0.5rem 2rem;
  }
}

.contributor-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.5rem 0.25rem;
  border-bottom: 1px solid var(--el-border-color-lighter);
}

.contributor-name {
  font-weight: 500;
  color: var(--el-text-color-regular);
}

.contributor-role {
  font-size: 0.9rem;
  color: var(--el-text-color-secondary);
}

/* Dependencies Two-Column Layout */
.deps-container {
  display: grid;
  grid-template-columns: 1fr;
  gap: 2rem;
}

@media (min-width: 768px) {
  .deps-container {
    grid-template-columns: 1fr 1fr;
    gap: 4rem;
  }
}

.deps-subtitle {
  font-size: 1rem;
  font-weight: 600;
  color: var(--el-text-color-secondary);
  margin-bottom: 1rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.deps-list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.dep-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.25rem 0;
}

.dep-name {
  font-size: 0.95rem;
}

.dep-license {
  font-size: 0.85rem;
  color: var(--el-text-color-secondary);
  background-color: var(--el-fill-color-lighter);
  padding: 1px 6px;
  border-radius: 4px;
}
</style>
