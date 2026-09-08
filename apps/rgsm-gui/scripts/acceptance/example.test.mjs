import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile, rm, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium, expect } from '@playwright/test';
import { spawnTestProcess } from '../../e2e/support/process.ts';
import { deviceLayout, readSave } from '../../e2e/support/cloud-fixture.ts';
import { createPacket } from '../../../../scripts/acceptance/create.mjs';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../../../..');

test(
  'generated example supports GUI A/B round trip, text panels and restart without reseeding',
  {
    skip: process.platform !== 'win32' && 'The runnable example is Windows-only',
    timeout: 600_000,
  },
  async (t) => {
    const root = await createPacket(`example-check-${Date.now()}`, { repoRoot, example: true });
    const entryFile = join(root, 'entrypoints.json');
    let owned;
    let output = '';
    let browser;
    let passed = false;
    async function start() {
      await rm(entryFile, { force: true });
      owned = spawnTestProcess(process.execPath, [join(root, 'run.mjs')], { cwd: repoRoot });
      owned.child.stdout.on('data', (data) => {
        output += data;
      });
      owned.child.stderr.on('data', (data) => {
        output += data;
      });
      let entries;
      await expect
        .poll(
          async () => {
            if (owned.failure())
              throw new Error(`Example failed: ${owned.failure().message}\n${output}`);
            try {
              entries = JSON.parse(await readFile(entryFile, 'utf8'));
              return entries.length;
            } catch (error) {
              if (error.code !== 'ENOENT') throw error;
              return 0;
            }
          },
          { timeout: 300_000 }
        )
        .toBe(2);
      return entries;
    }
    async function stop(panel, entries) {
      await panel.getByRole('button', { name: 'Stop entire session' }).click();
      await expect.poll(() => owned.child.exitCode, { timeout: 15_000 }).toBe(0);
      for (const entry of entries) {
        await assert.rejects(fetch(entry.gui, { signal: AbortSignal.timeout(1_000) }));
      }
      await assert.rejects(readFile(join(root, '.running')), { code: 'ENOENT' });
    }
    const saveA = deviceLayout(join(root, 'data'), 'a');
    const saveB = deviceLayout(join(root, 'data'), 'b');
    async function open(page, origin, path) {
      await page.goto(origin + path);
      await expect(
        page.getByRole('heading', {
          name: path === '/SyncSettings' ? 'Sync settings' : 'Echo Keep',
          exact: true,
        })
      ).toBeVisible();
    }
    function row(page, description) {
      return page.getByRole('row').filter({ has: page.getByText(description, { exact: true }) });
    }
    async function publish(page, origin, description) {
      await open(page, origin, '/Management/Echo%20Keep');
      await page.getByPlaceholder('New backup description').fill(description);
      await page.getByRole('button', { name: 'Create new snapshot' }).click();
      const snapshot = row(page, description);
      const upload = snapshot.getByRole('button', { name: 'Upload to cloud' });
      const uploaded = snapshot.getByRole('button', { name: 'Remove cloud copy' });
      const action = await upload
        .or(uploaded)
        .first()
        .getAttribute('aria-label', { timeout: 30_000 });
      if (action === 'Upload to cloud') await upload.click();
      await expect(uploaded).toBeVisible({ timeout: 30_000 });
    }
    async function restore(page, origin, description, device, contents) {
      await open(page, origin, '/Management/Echo%20Keep');
      const snapshot = row(page, description);
      await snapshot.getByRole('button', { name: 'Download to this device' }).click();
      await expect(snapshot.getByRole('button', { name: 'Remove from this device' })).toBeVisible({
        timeout: 30_000,
      });
      await snapshot.getByRole('button', { name: 'Apply', exact: true }).click();
      await expect.poll(() => readSave(device), { timeout: 30_000 }).toBe(contents);
    }
    try {
      const entries = await start();
      const [a, b] = entries;
      // Explicit headless launch, no desktop shell or personal browser profile.
      browser = await chromium.launch({ headless: true });
      const pageA = await browser.newPage();
      const pageB = await browser.newPage();
      const panelA = await browser.newPage();
      const panelB = await browser.newPage();
      const pageErrors = [];
      for (const page of [pageA, pageB, panelA, panelB])
        page.on('pageerror', (error) => pageErrors.push(error.message));
      await panelA.goto(a.panel);
      await expect(panelA.getByRole('textbox')).toHaveValue('A: starting progress\n');
      await panelB.goto(b.panel);
      await expect(panelB.getByRole('textbox')).toHaveValue('B: starting progress\n');
      assert.equal((await fetch(`${a.gui}/__acceptance/text`)).status, 401);
      assert.equal(
        (await fetch(a.panel, { headers: { Origin: 'https://example.invalid' } })).status,
        403
      );
      assert.equal((await fetch(`${a.gui}/api/v1/get-build-info`, { method: 'POST' })).status, 401);

      await open(pageA, a.gui, '/SyncSettings');
      await pageA.getByRole('button', { name: 'Create library', exact: true }).click();
      await expect(pageA.getByRole('textbox', { name: 'Search games' })).toBeVisible({
        timeout: 30_000,
      });
      await publish(pageA, a.gui, 'A checkpoint');
      await open(pageB, b.gui, '/SyncSettings');
      await expect(pageB.getByRole('textbox', { name: 'Search games' })).toBeVisible({
        timeout: 30_000,
      });
      await restore(pageB, b.gui, 'A checkpoint', saveB, 'A: starting progress\n');
      await panelB.getByRole('button', { name: 'Read saved file' }).click();
      await expect(panelB.getByRole('textbox')).toHaveValue('A: starting progress\n');
      await panelB.getByRole('textbox').fill('B: new progress');
      await panelB.getByRole('button', { name: 'Write edited text' }).click();
      await expect(panelB.getByRole('status')).toHaveText('Written to disk');
      assert.equal(await readSave(saveA), 'A: starting progress\n');
      await publish(pageB, b.gui, 'B checkpoint');
      await restore(pageA, a.gui, 'B checkpoint', saveA, 'B: new progress');
      await panelA.getByRole('button', { name: 'Read saved file' }).click();
      await expect(panelA.getByRole('textbox')).toHaveValue('B: new progress');
      await panelA.screenshot({ path: join(root, 'evidence/panel.png') });
      assert.deepEqual(pageErrors, []);
      await stop(panelA, entries);

      const resumed = await start();
      await panelA.goto(resumed[0].panel);
      await expect(panelA.getByRole('textbox')).toHaveValue('B: new progress');
      await open(pageA, resumed[0].gui, '/Management/Echo%20Keep');
      await expect(row(pageA, 'A checkpoint')).toBeVisible();
      await expect(row(pageA, 'B checkpoint')).toBeVisible();
      await stop(panelA, resumed);
      passed = true;
    } finally {
      await browser?.close();
      await owned?.stop();
      await writeFile(join(root, 'evidence/launcher.log'), output);
      if (passed) await rm(root, { recursive: true, force: true });
      else t.diagnostic(`Failure evidence retained at ${root}`);
    }
  }
);
