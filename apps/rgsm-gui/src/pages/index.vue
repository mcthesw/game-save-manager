<template>
  <div class="home-container">
    <button type="button" class="lang-chip" @click="langVisible = !langVisible">
      <Languages :size="14" />
      {{ currentLanguageName }}
      <ChevronDown :size="13" class="lang-chip-arrow" />
    </button>

    <div class="marquee-field" aria-hidden="true">
      <div
        v-for="(row, r) in wallRows"
        :key="r"
        class="marquee-row"
        :class="[row.tone, { reverse: row.reverse }]"
        :style="{
          '--dur': `${row.duration}s`,
          '--off': `${row.offset}s`,
          '--size': row.size,
          '--r': r,
        }"
      >
        <div class="marquee-track">
          <span v-for="copy in 2" :key="copy" class="marquee-copy">
            <span
              v-for="(word, w) in row.words"
              :key="w"
              class="marquee-word"
              :style="{ opacity: wordOpacity(r, w) }"
              >{{ word }}</span
            >
          </span>
        </div>
      </div>
    </div>

    <div class="focal-scrim"></div>
    <div class="focal">
      <img class="focal-logo" :src="appLogo" alt="" />
      <p class="focal-name">{{ $t('home.name') }}</p>
      <button v-if="appVersion" type="button" class="focal-version" @click="goAbout">
        v{{ appVersion }}<span v-if="gitHash"> ({{ gitHash }})</span>
      </button>
    </div>

    <section class="status-row">
      <button type="button" class="status-cell status-link" @click="goAddGame">
        <span class="status-value" :class="{ 'is-guide': gameCount === 0 }">
          {{ gameCount === 0 ? $t('home.add_first_game') : displayGameCount }}
        </span>
        <span class="status-label">{{ $t('home.status_games') }}</span>
      </button>
      <button type="button" class="status-cell status-link" @click="goSync">
        <span class="status-value" :class="{ 'is-guide': cloudBackend === 'Disabled' }">
          {{ cloudText }}
        </span>
        <span class="status-label">{{ $t('home.status_cloud') }}</span>
      </button>
      <button type="button" class="status-cell status-link" @click="goAutoBackup">
        <span class="status-value">{{ displayAutoBackupCount }}</span>
        <span class="status-label">{{ $t('home.status_auto_backup') }}</span>
      </button>
    </section>

    <div v-if="langVisible" class="lang-overlay" @click="langVisible = false"></div>
    <Transition name="lang-pop">
      <div v-if="langVisible" class="lang-panel" @click.stop>
        <p class="lang-title">{{ $t('home.choose_language') }}</p>
        <ul class="lang-list">
          <li
            v-for="lang in languages"
            :key="lang.code"
            class="lang-item"
            :class="{ active: lang.code === currentLocale }"
            @click="chooseLanguage(lang.code)"
          >
            <span>{{ lang.name }}</span>
            <Check v-if="lang.code === currentLocale" :size="14" />
          </li>
        </ul>
        <div class="lang-footer">
          <span class="translate-link" @click="openTranslate">
            {{ $t('home.help_translate') }}
          </span>
        </div>
      </div>
    </Transition>
  </div>
</template>

<script lang="ts" setup>
import { Check, ChevronDown, Languages } from '@lucide/vue';
import { usePreferredReducedMotion } from '@vueuse/core';
import { computed, onMounted, ref, watch } from 'vue';
import type { Ref } from 'vue';
import { error } from '../utils/logger';
import { commands } from '../api/commands';
import { getGameManagementPath } from '../composables/useGameManagementRoute';
import { useAddGameDrawer } from '../composables/useAddGameDrawer';
import { $t, getSupportedLanguages, i18n } from '../i18n';
// 与应用图标同源,避免在前端复制一份 logo 资产
import appLogo from '../../src-tauri/icons/icon.png';

const { config, saveConfig } = useConfig();
const { open: openAddGame } = useAddGameDrawer();

const languages = getSupportedLanguages();
const currentLocale = computed(() => i18n.global.locale.value);
const langVisible = ref(false);

type LocaleMessages = { home?: { greeting?: string } };
const allMessages = i18n.global.messages.value as Record<string, LocaleMessages>;

function greetingOf(code: string, fallback: string): string {
  return allMessages[code]?.home?.greeting || fallback;
}

// ——— 问候流场(环境纹理)———
// 14 行无缝跑马灯铺满整页,混排 8 种语言的问候词;行向/速度/起点错落,
// 词级透明度抖动去栅格感。它只是背景的「风」,不可点击,交互入口在右上角。
const ROW_COUNT = 14;
const CYCLES = 5;

// 0.82 → 1.0 → 0.82 的浅正弦梯度,整体保持均匀低幅
const ROW_SIZES = Array.from(
  { length: ROW_COUNT },
  (_, r) => Math.round((0.82 + 0.18 * Math.sin((Math.PI * r) / (ROW_COUNT - 1))) * 100) / 100
);

type RowTone = 'dim' | 'mid';
interface WallRow {
  words: string[];
  tone: RowTone;
  size: number;
  duration: number;
  offset: number;
  reverse: boolean;
}

const greetings = computed(() =>
  languages.map((l) => greetingOf(l.code, l.name)).filter((g) => g.length > 0)
);

const currentLanguageName = computed(() => {
  const lang = languages.find((l) => l.code === currentLocale.value);
  return lang?.name ?? currentLocale.value;
});

const appVersion = computed(() => config.value?.version ?? '');
const gitHash = ref('');
onMounted(async () => {
  try {
    const info = await commands.getBuildInfo();
    gitHash.value = info.git_hash;
  } catch {
    gitHash.value = '';
  }
});

function rowWords(rowIndex: number, words: string[]): string[] {
  const n = words.length;
  if (n === 0) return [];
  const out: string[] = [];
  for (let c = 0; c < CYCLES; c++) {
    for (let i = 0; i < n; i++) {
      // 每行错 3 位、每循环再错 1 位,打乱相邻关系
      out.push(words[(i + c + rowIndex * 3) % n] ?? '');
    }
  }
  return out;
}

// 确定性伪随机:同一位置每次渲染得到相同透明度
function wordOpacity(row: number, col: number): number {
  return 0.55 + ((row * 31 + col * 17) % 5) * 0.11;
}

const wallRows = computed<WallRow[]>(() =>
  ROW_SIZES.map((size, r) => ({
    words: rowWords(r, greetings.value),
    tone: r % 5 === 2 ? 'mid' : 'dim',
    size,
    duration: 40 + ((r * 13) % 31),
    offset: -((r * 7.3) % 40), // 负延时让各行起始位置错开
    reverse: r % 2 === 1,
  }))
);

// ——— 状态行 ———
const displayGameCount = ref(0);
const displayAutoBackupCount = ref(0);
const gameCount = computed(() => config.value?.games?.length ?? 0);

const reducedMotion = usePreferredReducedMotion();
const motionOff = computed(() => reducedMotion.value === 'reduce');

function countUp(target: number, out: Ref<number>) {
  if (motionOff.value) {
    out.value = target;
    return;
  }
  const from = out.value;
  const duration = 700;
  const t0 = performance.now();
  const tick = (t: number) => {
    const p = Math.min(1, (t - t0) / duration);
    out.value = Math.round(from + (target - from) * (1 - Math.pow(1 - p, 3)));
    if (p < 1) requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}

const cloudBackend = computed(
  () => config.value?.settings?.cloud_settings?.backend?.type ?? 'Disabled'
);
const cloudText = computed(() => {
  switch (cloudBackend.value) {
    case 'WebDAV':
      return 'WebDAV';
    case 'S3':
      return 'S3';
    case 'Fs':
      return $t('home.cloud_local');
    default:
      return $t('home.cloud_not_configured');
  }
});

onMounted(() => {
  countUp(gameCount.value, displayGameCount);
  void commands.getAutoBackupStatus().then((res) => {
    if (res.status === 'ok') countUp(res.data.length, displayAutoBackupCount);
  });
});

watch(gameCount, (v) => countUp(v, displayGameCount));

async function chooseLanguage(code: string) {
  langVisible.value = false;
  if (code === currentLocale.value) return;
  i18n.global.locale.value = code as typeof i18n.global.locale.value;
  config.value.settings.locale = code;
  await saveConfig();
  notifyInfo($t('settings.locale_changed'));
}

// 与设置页同入口:引导到贡献指南
async function openTranslate() {
  try {
    await commands.openUrl(
      'https://github.com/mcthesw/game-save-manager/blob/main/CONTRIBUTING.md'
    );
  } catch (e) {
    error(`open translate website error: ${e}`);
    notifyError($t('error.open_url_failed'));
  }
}

function goAddGame() {
  openAddGame();
}
function goSync() {
  navigateTo('/SyncSettings');
}
function goAutoBackup() {
  // 定时备份是按游戏配置的:有游戏去第一个游戏的管理页,没有则引导先导入
  const first = config.value?.games?.[0];
  if (first) {
    navigateTo(getGameManagementPath(first.name));
  } else {
    openAddGame();
  }
}
function goAbout() {
  navigateTo('/About');
}
</script>

<style scoped>
.home-container {
  position: relative;
  display: flex;
  flex-direction: column;
  font-family: var(--font-sans-stack);
  /* ElScrollbar__view 是自动高度,min-height:100% 会塌缩;
     直接抵消 el-main 默认 20px padding 并占满视口高度 */
  height: 100vh;
  margin: -20px;
  overflow: hidden;
}

/* 右上角语言芯片:语言切换与「帮助翻译」的可见入口 */
.lang-chip {
  position: absolute;
  top: 1.2rem;
  right: 1.5rem;
  z-index: 2;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 7px 14px;
  border: 1px solid color-mix(in srgb, var(--border) 65%, transparent);
  border-radius: 999px;
  background: color-mix(in srgb, var(--bg) 62%, transparent);
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
  font: inherit;
  font-size: 0.85rem;
  color: var(--text);
  cursor: pointer;
  transition:
    transform 0.25s ease,
    background-color 0.25s ease;
}

.lang-chip:hover {
  transform: translateY(-1px);
  background: color-mix(in srgb, var(--bg) 85%, transparent);
  color: var(--text);
}

.lang-chip:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

.lang-chip-arrow {
  font-size: 12px;
}

/* ——— 问候流场:环境纹理,铺满整页 ——— */
.marquee-field {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  justify-content: space-evenly;
  user-select: none;
}

.marquee-row {
  overflow: hidden;
  font-size: calc(var(--size) * 1rem);
  line-height: 1.25;
  white-space: nowrap;
  color: color-mix(in oklab, var(--text-dim) 75%, transparent);
  opacity: 0.38;
  transition: opacity 0.35s ease;
  animation: row-in 0.6s cubic-bezier(0.2, 0.7, 0.3, 1) both;
  animation-delay: calc(var(--r) * 55ms);
}

/* 悬停某行:该行暂停并提亮,可凑近看清 */
.marquee-row:hover {
  opacity: 0.62;
}

.marquee-row.mid {
  color: var(--text-dim);
}

.marquee-track {
  display: flex;
  width: max-content;
  animation: marquee var(--dur) linear infinite;
  animation-delay: var(--off);
}

.marquee-row.reverse .marquee-track {
  animation-direction: reverse;
}

.marquee-row:hover .marquee-track {
  animation-play-state: paused;
}

.marquee-copy {
  display: flex;
  align-items: baseline;
  gap: 0 1.3em;
  flex-shrink: 0;
  padding-right: 1.3em;
}

@keyframes marquee {
  from {
    transform: translateX(0);
  }
  to {
    transform: translateX(-50%);
  }
}

@keyframes row-in {
  from {
    opacity: 0;
    transform: translateY(22px);
  }
}

/* 中央焦点:图标 + 软件名 + 版本号,整体缓浮;不挡行悬停,仅版本号可点 */
.focal-scrim {
  position: absolute;
  inset: 0;
  background: radial-gradient(
    ellipse 38% 30% at 50% 46%,
    var(--bg) 0%,
    color-mix(in srgb, var(--bg) 72%, transparent) 55%,
    transparent 82%
  );
  pointer-events: none;
}

.focal {
  position: absolute;
  left: 50%;
  top: 45%;
  transform: translate(-50%, -50%);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.45rem;
  pointer-events: none;
  animation:
    focal-in 0.7s 0.2s cubic-bezier(0.2, 0.9, 0.3, 1.15) both,
    focal-float 7s ease-in-out 1.1s infinite;
}

.focal-logo {
  width: 88px;
}

.focal-name {
  margin: 0;
  font-size: 1.05rem;
  font-weight: 600;
  letter-spacing: 0.06em;
  color: var(--text);
}

.focal-version {
  margin: 0;
  padding: 2px 8px;
  border: none;
  border-radius: var(--radius-sm);
  background: none;
  font-family: var(--font-mono-stack);
  font-size: 0.8rem;
  color: var(--text-dim);
  cursor: pointer;
  pointer-events: auto;
  transition: color 0.2s ease;
}

.focal-version:hover {
  color: var(--text);
}

.focal-version:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

@keyframes focal-in {
  from {
    opacity: 0;
    transform: translate(-50%, -50%) scale(0.8);
  }
  to {
    opacity: 1;
    transform: translate(-50%, -50%) scale(1);
  }
}

@keyframes focal-float {
  0%,
  100% {
    transform: translate(-50%, -50%) translateY(-5px);
  }
  50% {
    transform: translate(-50%, -50%) translateY(5px);
  }
}

/* ——— 状态行:纯文字,直接坐在流场上 ——— */
.status-row {
  position: relative;
  z-index: 1;
  margin-top: auto;
  display: flex;
  justify-content: center;
  flex-wrap: wrap;
  gap: 1rem 4rem;
  padding: 2.5rem 1rem;
}

.status-cell {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.35rem;
  padding: 0.7rem 1.7rem;
  border: none;
  border-radius: var(--radius-md);
  /* 无边框玻璃片:与流动文字软隔离 */
  background: color-mix(in srgb, var(--bg) 58%, transparent);
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
  font: inherit;
}

.status-link {
  cursor: pointer;
  transition:
    transform 0.25s ease,
    background-color 0.25s ease;
}

.status-link:hover {
  transform: translateY(-2px);
  background: color-mix(in srgb, var(--bg) 82%, transparent);
}

.status-link:hover .status-value,
.status-link:hover .status-label {
  color: var(--text);
}

.status-link:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

.status-value {
  font-family: var(--font-mono-stack);
  font-size: 1.9rem;
  font-weight: 650;
  line-height: 1.2;
  color: var(--text);
  font-variant-numeric: tabular-nums;
  transition: color 0.25s ease;
}

.status-value.is-guide {
  color: var(--accent);
}

.status-label {
  font-size: 0.85rem;
  color: var(--text-dim);
  transition: color 0.25s ease;
}

/* ——— 语言选择面板 ——— */
.lang-overlay {
  position: fixed;
  inset: 0;
  z-index: 10;
}

.lang-panel {
  position: absolute;
  right: 1.5rem;
  top: 4.3rem;
  z-index: 11;
  min-width: 220px;
  padding: 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--surface);
  box-shadow: var(--shadow-overlay);
}

.lang-pop-enter-active,
.lang-pop-leave-active {
  transition:
    opacity 0.18s ease,
    transform 0.18s cubic-bezier(0.2, 0.9, 0.3, 1.15);
}

.lang-pop-enter-from,
.lang-pop-leave-to {
  opacity: 0;
  transform: translateY(-6px) scale(0.97);
}

.lang-title {
  margin: 0 0 6px;
  font-size: 0.8rem;
  color: var(--text-dim);
}

.lang-list {
  list-style: none;
  margin: 0;
  padding: 0;
}

.lang-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 7px 10px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  color: var(--text);
}

.lang-item:hover {
  background-color: var(--surface-2);
}

.lang-item.active {
  color: var(--text);
  font-weight: 600;
}

.lang-footer {
  margin-top: 6px;
  padding-top: 8px;
  border-top: 1px solid var(--border);
  text-align: center;
}

.translate-link {
  cursor: pointer;
  font-size: 0.85rem;
  color: var(--text-dim);
}

.translate-link:hover {
  color: var(--text);
  text-decoration: underline;
}

@media (prefers-reduced-motion: reduce) {
  .marquee-row,
  .marquee-track,
  .focal {
    animation: none;
  }

  .marquee-row {
    opacity: 0.45;
    transition: none;
  }

  .status-link:hover {
    transform: none;
  }

  .lang-pop-enter-active,
  .lang-pop-leave-active {
    transition: none;
  }
}

@media (max-width: 768px) {
  .marquee-row {
    font-size: calc(var(--size) * 0.8rem);
  }

  .focal-logo {
    width: 72px;
  }

  .status-row {
    gap: 0.8rem 1.2rem;
  }
}
</style>
