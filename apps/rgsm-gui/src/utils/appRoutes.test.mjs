import assert from 'node:assert/strict';
import test from 'node:test';

import {
  isValidAppDestination,
  mapLegacyHomePage,
  managementGameExists,
  resolveStartupDestination,
} from './appRoutes.ts';

const games = [{ name: 'Isaac' }, { name: 'Hollow Knight' }];

test('legacy AddGame homepage maps to home', () => {
  assert.equal(mapLegacyHomePage('/AddGame'), '/');
  assert.equal(mapLegacyHomePage('/Settings'), '/Settings');
  assert.equal(mapLegacyHomePage(''), '/');
});

test('management lookup decodes the route name', () => {
  assert.equal(managementGameExists(games, 'Hollow%20Knight'), true);
  assert.equal(managementGameExists(games, 'Missing'), false);
  assert.equal(managementGameExists(games, ''), false);
});

test('startup keeps a loaded management deep link', () => {
  assert.equal(
    resolveStartupDestination('/Management/Isaac', '/', games, 'Isaac'),
    '/Management/Isaac'
  );
  assert.equal(
    resolveStartupDestination('/Management/Hollow%20Knight', '/', games, 'Hollow%20Knight'),
    '/Management/Hollow%20Knight'
  );
});

test('startup does not keep an unknown management game', () => {
  assert.equal(
    resolveStartupDestination('/Management/Missing', '/Settings', games, 'Missing'),
    '/'
  );
});

test('startup keeps a refreshed settings page', () => {
  assert.equal(resolveStartupDestination('/Settings', '/', games), '/Settings');
});

test('startup remaps a leftover AddGame URL to home', () => {
  assert.equal(resolveStartupDestination('/AddGame', '/AddGame', games), '/');
});
test('startup applies the configured homepage from /', () => {
  assert.equal(resolveStartupDestination('/', '/About', games), '/About');
  assert.equal(resolveStartupDestination('/', '/AddGame', games), '/');
  assert.equal(resolveStartupDestination('/', '/Management/Isaac', games), '/Management/Isaac');
});

test('invalid configured homepage is a normal fallback to home', () => {
  assert.equal(isValidAppDestination('/AddGame', games), true);
  assert.equal(resolveStartupDestination('/', '/Nope', games), '/');
});
