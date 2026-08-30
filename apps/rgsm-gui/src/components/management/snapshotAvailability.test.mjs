import assert from 'node:assert/strict';
import test from 'node:test';

import {
  canApplySnapshot,
  canEvictSnapshot,
  canUploadSnapshot,
  snapshotAvailability,
  snapshotLocationKey,
} from './snapshotAvailability.ts';

function cloudGame(snapshots) {
  return { snapshots };
}

function cloudSnapshot(snapshotId, overrides = {}) {
  return {
    snapshot_id: snapshotId,
    local_evidence: 'unknown',
    cloud_verified: false,
    reported_on_devices: [],
    ...overrides,
  };
}

test('local-only snapshot is available on this device without a cloud catalog', () => {
  const localDates = new Set(['local']);

  assert.deepEqual(snapshotAvailability(localDates, null, 'local'), {
    onDevice: true,
    inCloud: false,
    onOtherDevice: false,
  });
  assert.equal(canApplySnapshot(localDates, null, 'local'), true);
  assert.equal(canEvictSnapshot(localDates, null, 'local'), false);
  assert.equal(snapshotLocationKey(localDates, null, 'local'), 'manage.location_local');
});

test('local catalog remains authoritative when the cloud catalog has no row yet', () => {
  const localDates = new Set(['local']);
  const game = cloudGame([]);

  assert.equal(snapshotAvailability(localDates, game, 'local').onDevice, true);
  assert.equal(canUploadSnapshot(localDates, game, 'local'), true);
  assert.equal(canEvictSnapshot(localDates, game, 'local'), true);
  assert.equal(snapshotLocationKey(localDates, game, 'local'), 'manage.location_local');
});

test('unknown cloud metadata does not override a local catalog row', () => {
  const localDates = new Set(['legacy']);
  const game = cloudGame([cloudSnapshot('legacy')]);

  assert.equal(snapshotAvailability(localDates, game, 'legacy').onDevice, true);
  assert.equal(canApplySnapshot(localDates, game, 'legacy'), true);
  assert.equal(snapshotLocationKey(localDates, game, 'legacy'), 'manage.location_local');
});

test('local mismatch evidence overrides a stale local catalog row', () => {
  const localDates = new Set(['corrupt']);
  const game = cloudGame([cloudSnapshot('corrupt', { local_evidence: 'mismatch' })]);

  assert.equal(snapshotAvailability(localDates, game, 'corrupt').onDevice, false);
  assert.equal(canApplySnapshot(localDates, game, 'corrupt'), false);
  assert.equal(snapshotLocationKey(localDates, game, 'corrupt'), 'manage.location_missing');
});

test('location key represents verified local, cloud, and other-device copies', () => {
  const localDates = new Set(['both']);
  const game = cloudGame([
    cloudSnapshot('both', { local_evidence: 'present', cloud_verified: true }),
    cloudSnapshot('cloud', { cloud_verified: true }),
    cloudSnapshot('elsewhere', { reported_on_devices: ['device-b'] }),
  ]);

  assert.equal(snapshotLocationKey(localDates, game, 'both'), 'manage.location_both');
  assert.equal(snapshotLocationKey(localDates, game, 'cloud'), 'manage.location_cloud');
  assert.equal(
    snapshotLocationKey(localDates, game, 'elsewhere'),
    'sync_settings.archives.available_other_device'
  );
});
