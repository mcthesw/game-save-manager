import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

function collectIds(suites) {
  return suites.flatMap((suite) => [
    ...(suite.specs ?? []).map((spec) => spec.id),
    ...collectIds(suite.suites ?? []),
  ]);
}

function listTests(shard, project) {
  // --list discovers real test files without building or starting any hosts.
  const result = spawnSync(
    process.execPath,
    [
      resolve(appRoot, 'node_modules/@playwright/test/cli.js'),
      'test',
      '--list',
      '--reporter=json',
      ...(project ? [`--project=${project}`] : []),
      ...(shard ? [`--shard=${shard}`] : []),
    ],
    { cwd: appRoot, encoding: 'utf8', windowsHide: true, maxBuffer: 4 * 1024 * 1024 }
  );
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.deepEqual(report.errors, []);
  return collectIds(report.suites);
}

test('two CI shards cover every discovered E2E exactly once', () => {
  const all = listTests(undefined, 'browser');
  const first = listTests('1/2', 'browser');
  const second = listTests('2/2', 'browser');
  assert.ok(first.length > 0 && second.length > 0);
  assert.equal(new Set([...first, ...second]).size, first.length + second.length);
  assert.deepEqual([...first, ...second].sort(), all.sort());
});

test('browser and desktop projects partition the complete suite', () => {
  const all = listTests();
  const browser = listTests(undefined, 'browser');
  const desktop = listTests(undefined, 'desktop');
  assert.ok(browser.length > 0 && desktop.length > 0);
  assert.equal(new Set([...browser, ...desktop]).size, browser.length + desktop.length);
  assert.deepEqual([...browser, ...desktop].sort(), all.sort());
});

test('the ordinary E2E command selects only the browser project', () => {
  const { scripts } = JSON.parse(readFileSync(resolve(appRoot, 'package.json'), 'utf8'));
  assert.equal(scripts['web:e2e'], 'playwright test --project=browser');
  assert.equal(scripts['web:e2e:desktop'], 'playwright test --project=desktop');
});
