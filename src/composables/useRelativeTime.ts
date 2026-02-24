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

// Interval handle for periodic refresh (shared across all composable consumers)
let refreshInterval: ReturnType<typeof setInterval> | null = null;

// Track active consumers so we can stop the timer when nobody needs it.
// Using a Set avoids negative counts / double-dispose edge cases.
const activeConsumers = new Set<symbol>();

// Reactive tick counter — increments every minute so any render/computed that reads it re-evaluates
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

// Optional cache to avoid repeatedly parsing identical snapshot date strings.
// Cleared automatically when the last consumer disposes.
const parsedCache = new Map<string, dayjs.Dayjs>();

function parseSnapshotDate(dateStr: string): dayjs.Dayjs | null {
  const cached = parsedCache.get(dateStr);
  if (cached) return cached;

  const parsed = dayjs(dateStr, DATE_FORMAT);
  if (!parsed.isValid()) return null;

  parsedCache.set(dateStr, parsed);
  return parsed;
}

export function useRelativeTime() {
  const { config } = useConfig();

  if (import.meta.client) {
    const token = Symbol('useRelativeTime');
    const wasEmpty = activeConsumers.size === 0;
    activeConsumers.add(token);
    if (wasEmpty) startInterval();

    onScopeDispose(() => {
      activeConsumers.delete(token);
      if (activeConsumers.size === 0) {
        stopInterval();
        parsedCache.clear();
      }
    });
  }

  // Resolve the dayjs locale from the app locale setting
  const dayjsLocale = computed(() => {
    const appLocale = config.value.settings.locale ?? 'en_US';
    return LOCALE_MAP[appLocale] ?? 'en';
  });

  /**
   * Given a snapshot date string (YYYY-MM-DD_HH-mm-ss), returns a relative-time
   * string such as "2 hours ago" / "2小时前".
   *
   * This function is reactive: it reads a shared `tick` ref that updates every
   * minute, so components/templates that call it will re-render periodically.
   *
   * Unlike the previous implementation, it does NOT create a new computed ref
   * per call, which avoids allocating many reactive effects in large tables.
   */
  function fromNow(dateStr: string): string {
    // Reading tick.value creates a reactive dependency so callers update every minute
    void tick.value;

    const parsed = parseSnapshotDate(dateStr);
    if (!parsed) return dateStr;

    return parsed.locale(dayjsLocale.value).fromNow();
  }

  return { fromNow };
}
