import { onUnmounted, ref, watch } from 'vue';
import type { SnapshotTimeFormat } from '../api/generated/types.gen';
import { $t } from '../i18n';
import { formatRelativeSnapshotTime, formatSnapshotTime } from '../utils/snapshotPresentation';

/** A list shares one local clock; changing its labels never reloads snapshot data. */
export function useSnapshotTime(format: () => SnapshotTimeFormat | undefined) {
  const now = ref(Date.now());
  let timer: ReturnType<typeof setInterval> | undefined;
  function stopClock() {
    if (timer !== undefined) clearInterval(timer);
    timer = undefined;
  }
  watch(
    format,
    (value) => {
      stopClock();
      if (value !== 'relative') return;
      now.value = Date.now();
      timer = setInterval(() => (now.value = Date.now()), 60_000);
    },
    { immediate: true }
  );
  onUnmounted(stopClock);

  type SnapshotTime = Parameters<typeof formatSnapshotTime>[0];
  const exactTimeLabel = (snapshot: SnapshotTime) =>
    formatSnapshotTime(snapshot) ?? $t('manage.unknown_snapshot_time');
  function timeLabel(snapshot: SnapshotTime) {
    if (format() !== 'relative') return exactTimeLabel(snapshot);
    return (
      formatRelativeSnapshotTime(snapshot, now.value, {
        justNow: $t('manage.snapshot_time_just_now'),
        minutesAgo: (count) =>
          $t(count === 1 ? 'manage.snapshot_time_minute_ago' : 'manage.snapshot_time_minutes_ago', {
            count,
          }),
        hoursAgo: (count) =>
          $t(count === 1 ? 'manage.snapshot_time_hour_ago' : 'manage.snapshot_time_hours_ago', {
            count,
          }),
        yesterday: (time) => $t('manage.snapshot_time_yesterday', { time }),
      }) ?? $t('manage.unknown_snapshot_time')
    );
  }
  return { timeLabel, exactTimeLabel };
}
