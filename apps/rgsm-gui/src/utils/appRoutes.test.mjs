import assert from 'node:assert/strict';
import test from 'node:test';

import {
  isValidAppDestination,
  mapLegacyHomePage,
  managementGameExists,
  resolveStartupDestination,
  resolveGameReference,
  resolveManagementGame,
  getGameManagementPath,
} from './appRoutes.ts';

const games = [{ name: 'Isaac' }, { name: 'Hollow Knight' }];

test('explicit references never fall back to another games name', () => {
  const first = { name: 'second', storage_key: 'first' };
  const second = { name: 'Title', storage_key: 'second' };
  assert.equal(resolveGameReference([first, second], 'second', 'second'), second);
  assert.equal(resolveGameReference([first], 'second', 'second'), undefined);
  assert.equal(resolveGameReference([first, second], 'second'), first);
});

test('route encoding preserves percent signs and reserved characters exactly once', () => {
  const game = { name: 'Save %2F / ? #', storage_key: 'key&one' };
  const link = getGameManagementPath(game);
  assert.equal(resolveManagementGame([game], link), game);
  assert.equal(resolveManagementGame([game], `/Management/${encodeURIComponent(game.name)}`), game);
  assert.equal(isValidAppDestination('/Management/Title/extra?gameId=key%26one', [game]), false);
});

test('same-title games require an explicit identity in management routes', () => {
  const duplicates = [
    { name: 'Echo Keep', storage_key: 'echo-a' },
    { name: 'Echo Keep', storage_key: 'echo-b' },
  ];
  assert.equal(isValidAppDestination('/Management/Echo%20Keep', duplicates), false);
  assert.equal(isValidAppDestination('/Management/Echo%20Keep?gameId=echo-b', duplicates), true);
  assert.equal(isValidAppDestination('/Management/Echo%20Keep?gameId=missing', duplicates), false);
  assert.equal(isValidAppDestination('/Management/Echo%20Keep?gameId=', duplicates), false);
});

test('startup retains stable identity after a game is renamed', () => {
  const renamed = [{ name: 'New title', storage_key: 'echo-a' }];
  const link = '/Management/Old%20title?gameId=echo-a';
  assert.equal(resolveStartupDestination(link, '/', renamed), link);
  assert.equal(resolveStartupDestination('/', link, renamed), link);
});

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
