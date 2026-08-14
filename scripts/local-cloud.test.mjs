import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { emptyLocalCloud, seedDocuments, seedLocalCloud } from './local-cloud.mjs';

test('empty leaves a blank location', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'rgsm-local-cloud-'));
  try {
    await seedLocalCloud(root);
    await emptyLocalCloud(root);
    await assert.rejects(readFile(path.join(root, 'v2', 'namespace.json')));
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('seed writes a two-game V2 library', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'rgsm-local-cloud-'));
  try {
    await seedLocalCloud(root);
    const documents = seedDocuments();
    for (const [relativePath, document] of Object.entries(documents)) {
      const raw = await readFile(path.join(root, relativePath), 'utf8');
      assert.deepEqual(JSON.parse(raw), document);
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
