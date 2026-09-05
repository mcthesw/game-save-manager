import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { buildEnvironment, prepareBuild, preparedBinary } from './build-state.ts';

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'rgsm-build-state-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const paths = { binary: join(root, 'rgsm'), helper: join(root, 'xxh3') };
  const build = () => {
    writeFileSync(paths.binary, 'host');
    writeFileSync(paths.helper, 'helper');
  };
  return { paths, build };
}

test('setup prepares once and fresh worker environments reuse both artifacts', (t) => {
  const { paths, build } = fixture(t);
  const env = {};
  let builds = 0;
  prepareBuild(env, paths, () => {
    builds++;
    build();
  });
  assert.equal(preparedBinary({ ...env }, paths), paths.binary);
  assert.equal(preparedBinary({ ...env }, paths), paths.binary);
  assert.equal(builds, 1);
});

test('every new setup revalidates source through a build, even with old artifacts', (t) => {
  const { paths, build } = fixture(t);
  const env = {};
  let builds = 0;
  const run = () => {
    builds++;
    build();
  };
  prepareBuild(env, paths, run);
  prepareBuild(env, paths, run);
  assert.equal(builds, 2);
});

test('existing binaries alone cannot bypass setup', (t) => {
  const { paths, build } = fixture(t);
  build();
  assert.throws(() => preparedBinary({}, paths), /global setup/);
});

test('failed rebuild invalidates previously prepared artifacts', (t) => {
  const { paths, build } = fixture(t);
  const env = {};
  prepareBuild(env, paths, build);
  assert.throws(() =>
    prepareBuild(env, paths, () => {
      throw new Error('compile failed');
    })
  );
  assert.throws(() => preparedBinary(env, paths), /global setup/);
});

test('partial or missing artifacts cannot be used by a worker', (t) => {
  const { paths, build } = fixture(t);
  const env = {};
  assert.throws(() => prepareBuild(env, paths, () => writeFileSync(paths.binary, 'host')));
  assert.throws(() => preparedBinary(env, paths), /global setup/);
  prepareBuild(env, paths, build);
  rmSync(paths.helper);
  assert.throws(() => preparedBinary(env, paths), /ENOENT/);
});

test('another checkout cannot reuse this run artifacts', (t) => {
  const { paths, build } = fixture(t);
  const env = {};
  prepareBuild(env, paths, build);
  assert.throws(
    () => preparedBinary(env, { ...paths, binary: paths.binary + '-other' }),
    /global setup/
  );
});

test('local builds default to two jobs without changing the caller environment', () => {
  const env = { PATH: 'tools' };
  assert.deepEqual(buildEnvironment(env), { PATH: 'tools', CARGO_BUILD_JOBS: '2' });
  assert.deepEqual(env, { PATH: 'tools' });
});

test('explicit job limits and CI runner defaults are preserved', () => {
  assert.equal(buildEnvironment({ CARGO_BUILD_JOBS: '1' }).CARGO_BUILD_JOBS, '1');
  assert.equal(buildEnvironment({ CI: 'true' }).CARGO_BUILD_JOBS, undefined);
  assert.equal(buildEnvironment({ CI: 'true', CARGO_BUILD_JOBS: '3' }).CARGO_BUILD_JOBS, '3');
});
