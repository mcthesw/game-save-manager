import assert from 'node:assert/strict';
import test from 'node:test';

import { saveUnitType } from './saveUnit.ts';

test('manifest save unit preserves a declared legacy type', () => {
  assert.equal(
    saveUnitType({
      source: {
        type: 'manifestPattern',
        expected_type: 'Folder',
        pattern: '<root>/Saved',
        constraints: { alternatives: [] },
      },
    }),
    'Folder'
  );
});

test('untyped manifest save unit defers its kind to resolved locations', () => {
  assert.equal(
    saveUnitType({
      source: {
        type: 'manifestPattern',
        pattern: '<home>/*.sav',
        constraints: { alternatives: [] },
      },
    }),
    undefined
  );
});
