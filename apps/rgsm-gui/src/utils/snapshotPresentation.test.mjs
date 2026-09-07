import assert from 'node:assert/strict';
import test from 'node:test';
import {
  compareSnapshotTime,
  formatSnapshotTime,
  formatRelativeSnapshotTime,
  findSnapshotByInput,
  snapshotDeviceName,
} from './snapshotPresentation.ts';

const relativeLabels = {
  justNow: 'Just now',
  minutesAgo: (count) => `${count} minutes ago`,
  hoursAgo: (count) => `${count} hours ago`,
  yesterday: (time) => `Yesterday ${time}`,
};
const relativeTime = (date, now) =>
  formatRelativeSnapshotTime({ date }, new Date(now).getTime(), relativeLabels);

test('relative time uses elapsed minutes, today hours, then calendar dates', () => {
  const now = '2026-09-07T14:30:00';
  assert.equal(relativeTime('2026-09-07_14-29-01', now), 'Just now');
  assert.equal(relativeTime('2026-09-07_14-29-00', now), '1 minutes ago');
  assert.equal(relativeTime('2026-09-07_13-31-00', now), '59 minutes ago');
  assert.equal(relativeTime('2026-09-07_13-30-00', now), '1 hours ago');
  assert.equal(relativeTime('2026-09-07_12-31-00', now), '1 hours ago');
  assert.equal(relativeTime('2026-09-06_22-15-42', now), 'Yesterday 22:15');
  assert.equal(relativeTime('2026-09-04_20-08-11', now), '09-04 20:08');
  assert.equal(relativeTime('2025-09-04_20-08-11', now), '2025-09-04 20:08');
});

test('recent minutes cross midnight while yesterday follows local calendar boundaries', () => {
  assert.equal(relativeTime('2026-09-06_23-50-00', '2026-09-07T00:10:00'), '20 minutes ago');
  assert.equal(relativeTime('2026-09-06_23-10-00', '2026-09-07T00:10:00'), 'Yesterday 23:10');
  assert.equal(relativeTime('2025-12-31_22-10-00', '2026-01-01T00:10:00'), 'Yesterday 22:10');
});

test('future and unknown times never pretend to be recent snapshots', () => {
  const now = new Date('2026-09-07T14:30:00').getTime();
  assert.equal(formatRelativeSnapshotTime({ date: 'opaque' }, now, relativeLabels), null);
  assert.equal(relativeTime('2026-09-07_14-31-00', '2026-09-07T14:30:00'), '2026-09-07 14:31:00');
  assert.equal(
    formatRelativeSnapshotTime({ date: 'opaque', created_at: now - 120000 }, now, relativeLabels),
    '2 minutes ago'
  );
});

test('opaque identities do not determine display time or chronological order', () => {
  const older = { date: 'zzzz', created_at: 1000 };
  const newer = { date: 'aaaa', created_at: 2000 };
  assert.ok(compareSnapshotTime(older, newer) < 0);
  assert.equal(formatSnapshotTime(newer, 'ss.SSS'), '02.000');
  assert.equal(compareSnapshotTime(older, { date: 'aaaa', created_at: 1000 }), 0);
  assert.equal(formatSnapshotTime({ date: 'opaque' }), null);
});

test('historical timestamp identities are displayed without changing their identity', () => {
  const legacy = { date: '2025-02-14_12-34-56' };
  assert.equal(formatSnapshotTime(legacy), '2025-02-14 12:34:56');
  assert.equal(legacy.date, '2025-02-14_12-34-56');
  assert.equal(formatSnapshotTime({ date: '2025-02-31_12-00-00' }), null);
  assert.equal(formatSnapshotTime({ date: '2025-02-14_12-34-56-extra' }), null);
  assert.equal(formatSnapshotTime({ date: 'opaque', created_at: 1e30 }), null);
});

test('creator labels never substitute the current holder for an unknown creator', () => {
  const devices = { a: { name: 'Desktop' }, b: { name: 'Handheld' } };
  assert.equal(snapshotDeviceName('a', devices), 'Desktop');
  assert.equal(snapshotDeviceName('a', { a: { name: 'Renamed' } }), 'Renamed');
  assert.equal(snapshotDeviceName(null, devices), null);
  assert.equal(snapshotDeviceName('unlisted-device', devices), 'unlisted…');
  assert.equal(snapshotDeviceName('a', undefined), 'a');
  assert.equal(snapshotDeviceName('constructor', devices), 'construc…');
});

test('equal timestamps on two devices stay separate and preserve input ordering', () => {
  const snapshots = [
    { date: 'z', created_at: 1234, device_id: 'a' },
    { date: 'a', created_at: 1234, device_id: 'b' },
    { date: 'legacy', created_at: 1000, device_id: null },
  ];
  assert.deepEqual(
    snapshots.sort(compareSnapshotTime).map((snapshot) => snapshot.date),
    ['legacy', 'z', 'a']
  );
});

test('parent selection treats digit-leading IDs as identities, never partial integers', () => {
  const snapshots = [
    { date: 'old', created_at: 1000, describe: 'first' },
    { date: '3e8af535-f31d-428b-a6bd-7e55cbd45c1f', created_at: 2000, describe: 'second' },
    { date: 'new', created_at: 3000, describe: 'third' },
  ];
  assert.equal(findSnapshotByInput(snapshots, snapshots[1].date), snapshots[1]);
  assert.equal(findSnapshotByInput(snapshots, '3e8af'), snapshots[1]);
  assert.equal(findSnapshotByInput(snapshots, '1'), snapshots[2]);
  assert.equal(findSnapshotByInput(snapshots, '3junk'), null);
});

test('parent selection accepts displayed time and only unique description or prefix matches', () => {
  const a = { date: 'a-one', created_at: 1000, describe: 'same description' };
  const b = { date: 'a-two', created_at: 2000, describe: 'same description' };
  assert.equal(findSnapshotByInput([a, b], formatSnapshotTime(a)), a);
  assert.equal(findSnapshotByInput([a, b], 'same'), null);
  assert.equal(findSnapshotByInput([a, b], 'a-'), null);
  b.created_at = a.created_at;
  assert.equal(findSnapshotByInput([a, b], formatSnapshotTime(a)), null);
  assert.equal(findSnapshotByInput([a, b], '1'), b);
});
