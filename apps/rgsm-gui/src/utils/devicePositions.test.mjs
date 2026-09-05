import assert from 'node:assert/strict';
import test from 'node:test';
import { devicePositions } from './devicePositions.ts';

test('local position wins, including an explicitly cleared position', () => {
  assert.deepEqual(devicePositions({}, {}, 'constructor'), {});
  assert.deepEqual(devicePositions({ pc: 'new' }, { pc: 'stale', deck: 'remote' }, 'pc'), {
    deck: 'remote',
    pc: 'new',
  });
  assert.deepEqual(devicePositions({}, { pc: 'stale', deck: 'remote' }, 'pc'), { deck: 'remote' });
});

test('cloud positions supersede copied positions but an unavailable cloud retains copied history', () => {
  assert.deepEqual(devicePositions({ deck: 'copied' }, {}, 'pc'), {});
  assert.deepEqual(devicePositions({ deck: 'copied' }, undefined, 'pc'), { deck: 'copied' });
  const special = JSON.parse('{"__proto__":"special","pc":"stale"}');
  assert.deepEqual(Object.entries(devicePositions({}, special, 'pc')), [['__proto__', 'special']]);
});
