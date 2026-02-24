import dayjs from 'dayjs';
import relativeTime from 'dayjs/plugin/relativeTime';
import 'dayjs/locale/zh-cn';
import 'dayjs/locale/fr';
import 'dayjs/locale/ko';
import 'dayjs/locale/nb';
import 'dayjs/locale/uk';

dayjs.extend(relativeTime);

// Map app locale codes to dayjs locale codes
const LOCALE_MAP: Record<string, string> = {
  zh_SIMPLIFIED: 'zh-cn',
  en_US: 'en',
  fr: 'fr',
  ko: 'ko',
  nb_NO: 'nb',
  uk: 'uk',
  ta: 'en', // Tamil not bundled in dayjs, fall back to English
};

const DATE_FORMAT = 'YYYY-MM-DD_HH-mm-ss';

// Interval handle for periodic refresh
let refreshInterval: ReturnType<typeof setInterval> | null = null;
let instanceCount = 0;

// Reactive tick counter — increments every minute so computed values re-evaluate
const tick = ref(0);

function startInterval() {
  if (refreshInterval !== null) return;
  refreshInterval = setInterval(() => {
    tick.value++;
  }, 60_000);
}

function stopInterval() {
  if (refreshInterval !== null) {
    clearInterval(refreshInterval);
    refreshInterval = null;
  }
}

export function useRelativeTime() {
  const { config } = useConfig();

  // Resolve the dayjs locale from the app locale setting
  const dayjsLocale = computed(() => {
    const appLocale = config.value.settings.locale ?? 'en_US';
    return LOCALE_MAP[appLocale] ?? 'en';
  });

  /**
   * Given a snapshot date string (YYYY-MM-DD_HH-mm-ss), returns a reactive
   * relative-time string such as "2 hours ago" / "2小时前".
   *
   * The returned value updates automatically every minute.
   */
  function fromNow(dateStr: string): ComputedRef<string> {
    return computed(() => {
      // Reading tick.value creates a reactive dependency so this re-computes every minute
      void tick.value;

      const parsed = dayjs(dateStr, DATE_FORMAT);
      if (!parsed.isValid()) return dateStr;

      return parsed.locale(dayjsLocale.value).fromNow();
    });
  }

  onMounted(() => {
    instanceCount++;
    startInterval();
  });

  onUnmounted(() => {
    instanceCount--;
    if (instanceCount <= 0) {
      instanceCount = 0;
      stopInterval();
    }
  });

  return { fromNow };
}
