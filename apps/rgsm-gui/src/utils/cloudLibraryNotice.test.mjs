import assert from 'node:assert/strict';
import test from 'node:test';
import { cloudLibraryNotice } from './cloudLibraryNotice.ts';

test('empty locations offer creation without inventing a failed attempt', () => {
  assert.deepEqual(cloudLibraryNotice({ kind: 'empty' }, false, false), {
    messageKey: 'sync_settings.library.empty',
    actionKey: 'sync_settings.library.create',
    action: 'create',
  });
});

test('only a failed creation offers retry, and reinspection can clear it', () => {
  const status = { kind: 'empty' };
  assert.equal(
    cloudLibraryNotice(status, false, true).actionKey,
    'sync_settings.library.retry_create'
  );
  assert.equal(
    cloudLibraryNotice(status, false, true).messageKey,
    'sync_settings.library.create_failed'
  );
  assert.equal(cloudLibraryNotice(status, false, false).messageKey, 'sync_settings.library.empty');
});

test('failed inspection cannot claim the previously inspected location is empty', () => {
  assert.equal(cloudLibraryNotice({ kind: 'empty' }, true, false).action, 'inspect');
  assert.equal(
    cloudLibraryNotice(null, true, false).messageKey,
    'sync_settings.library.inspect_failed'
  );
});

test('removed devices and replaced libraries have one specific recovery action', () => {
  assert.equal(
    cloudLibraryNotice({ kind: 'reconnect_required' }, false, false).action,
    'reconnect'
  );
  assert.equal(cloudLibraryNotice({ kind: 'rebuild_required' }, false, false).action, 'rebuild');
});

test('active, joining and upgrading libraries do not display setup error cards', () => {
  for (const kind of ['active', 'join_required', 'cutover_required']) {
    assert.equal(cloudLibraryNotice({ kind }, false, true), null);
  }
});

test('an inspection without a result does not flash a setup warning', () => {
  assert.equal(cloudLibraryNotice(null, false, false), null);
});
