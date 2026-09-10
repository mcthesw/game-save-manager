import assert from 'node:assert/strict';
import test from 'node:test';
import { initialGameListView } from './sidebarView.ts';

test('old configurations keep the existing favorite-based default', () => {
  assert.equal(initialGameListView(undefined, true), 'favorites');
  assert.equal(initialGameListView(null, false), 'all');
});

test('an explicit preference wins even for an empty favorite list', () => {
  assert.equal(initialGameListView('favorites', false), 'favorites');
  assert.equal(initialGameListView('all', true), 'all');
});
