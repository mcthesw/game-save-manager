import assert from 'node:assert/strict';
import test from 'node:test';

import { resolveCloudUiMode } from './cloudNamespace.ts';

test('local V2 activation keeps legacy controls disabled while remote status is unavailable', () => {
  assert.equal(resolveCloudUiMode('v2'), 'v2');
});

test('unknown local generation fails closed instead of exposing legacy controls', () => {
  assert.equal(resolveCloudUiMode(null), 'loading');
});

test('only an explicit legacy generation exposes legacy controls', () => {
  assert.equal(resolveCloudUiMode('legacy_v1'), 'legacy');
});
