import { test, expect } from '@playwright/test';
import AdmZip from 'adm-zip';
import { access, mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import { createRunRoot } from './support/rgsm-instance';
import { applySnapshotViaApi } from './support/local-gui';
import { DEVICE_A_ID, GAME_NAME } from './support/constants';

function legacyZip(entry = 'profile.sav') {
  const zip = new AdmZip();
  zip.addFile(entry, Buffer.from('historical-progress'));
  return zip.toBuffer();
}

test('local upgrades pause, resume, repair pending entries and retain originals until cleanup', async ({
  browser,
}, testInfo) => {
  const runRoot = await createRunRoot('local-archive-upgrade');
  const saves = join(runRoot, 'long install directory for checking narrow layouts', 'Saves');
  const profile = join(saves, 'profile.sav');
  const preferences = join(saves, 'preferences.sav');
  const device = await seedLocalConfig(runRoot, {
    games: [
      {
        name: GAME_NAME,
        units: [
          { type: 'File', path: profile },
          { type: 'File', path: preferences },
        ],
      },
    ],
    settings: { extra_backup_when_apply: false },
  });
  await writeSaveText(profile, 'live-progress');
  await writeSaveText(preferences, 'live-preferences');
  const directory = join(device.archiveRoot, GAME_NAME);
  await mkdir(directory, { recursive: true });
  const ids = ['2025-01-02_03-04-05', 'missing', 'damaged', 'association'];
  await writeFile(join(directory, `${ids[0]}.zip`), legacyZip());
  await writeFile(join(directory, 'damaged.zip'), 'damaged archive');
  await writeFile(join(directory, 'association.zip'), legacyZip('old-preferences.sav'));
  const backups = ids.map((date, index) => ({
    date,
    describe: `note-${date}`,
    path: join(directory, `${date}.zip`),
    parent: index ? ids[index - 1] : null,
  }));
  const catalogPath = join(directory, 'Backups.json');
  await writeFile(
    catalogPath,
    JSON.stringify({
      name: GAME_NAME,
      backups,
      device_heads: { [DEVICE_A_ID]: ids[0] },
      sync_version: 3,
    })
  );
  const replacement = join(runRoot, 'intact copy of the missing or damaged backup.zip');
  const replacementBytes = legacyZip();
  await writeFile(replacement, replacementBytes);
  const session = await startLocalSession(browser, { runRoot, device, label: 'local-upgrade' });
  let failed = false;
  try {
    const { page, host } = session;
    await page.goto('/Settings');
    await page.getByRole('button', { name: 'Backup settings', exact: true }).click();
    const panel = page.getByTestId('local-upgrade');
    await expect(panel.getByText('4 older backups', { exact: true })).toBeVisible();
    await expect(access(join(device.archiveRoot, '.rgsm-upgrade'))).rejects.toThrow();
    await page.screenshot({
      path: testInfo.outputPath('upgrade-preview.png'),
      animations: 'disabled',
    });

    // Delay delivery of one real backend response to exercise Pause without
    // substituting a mocked conversion or racing a tiny fixture archive.
    let hold = true;
    const converted = Promise.withResolvers<void>();
    const release = Promise.withResolvers<void>();
    await page.route('**/api/v1/local-archive-upgrade', async (route) => {
      if (hold && route.request().postDataJSON().action === 'step') {
        hold = false;
        const response = await route.fetch();
        converted.resolve();
        await release.promise;
        await route.fulfill({ response });
      } else await route.continue();
    });
    await panel.getByRole('button', { name: 'Start upgrade', exact: true }).click();
    await converted.promise;
    await panel.getByRole('button', { name: 'Pause', exact: true }).click();
    release.resolve();
    await expect(
      panel.getByRole('button', { name: 'Continue upgrade', exact: true })
    ).toBeVisible();
    await page.reload();
    await page.getByRole('button', { name: 'Backup settings', exact: true }).click();
    await expect(panel.getByText('1 of 4 backups upgraded', { exact: true })).toBeVisible();
    await panel.getByRole('button', { name: 'Continue upgrade', exact: true }).click();
    await expect(panel.getByText('3 items need attention', { exact: true })).toBeVisible();
    expect(await readFile(profile, 'utf8')).toBe('live-progress');
    expect(await readFile(preferences, 'utf8')).toBe('live-preferences');

    const ambiguous = panel.locator('details').filter({ hasText: 'association.zip' });
    await ambiguous.locator('summary').click();
    await expect(ambiguous.getByRole('button', { name: 'Retry', exact: true })).toBeDisabled();
    await page.setViewportSize({ width: 800, height: 850 });
    await page.screenshot({
      path: testInfo.outputPath('upgrade-pending-narrow.png'),
      animations: 'disabled',
    });
    const select = ambiguous.getByRole('combobox', { name: 'Choose save entry' });
    await select.click();
    await page.getByRole('option', { name: /^2\./ }).click();
    await ambiguous.getByRole('button', { name: 'Retry', exact: true }).click();
    await expect(panel.getByText('2 of 4 backups upgraded', { exact: true })).toBeVisible();
    for (const id of ['missing', 'damaged']) {
      const pending = panel.locator('details').filter({ hasText: `${id}.zip` });
      await pending.locator('summary').click();
      await pending.getByRole('textbox', { name: 'Backup copy (optional)' }).fill(replacement);
      await pending.getByRole('button', { name: 'Retry', exact: true }).click();
      await expect(pending).toHaveCount(0);
    }
    await expect(panel.getByText('4 of 4 backups upgraded', { exact: true })).toBeVisible();
    const catalog = JSON.parse(await readFile(catalogPath, 'utf8'));
    expect(catalog.backups.map((s: { date: string }) => s.date)).toEqual(ids);
    expect(catalog.backups.map((s: { parent?: string }) => s.parent ?? null)).toEqual([
      null,
      ...ids.slice(0, -1),
    ]);
    expect(catalog.device_heads[DEVICE_A_ID]).toBe(ids[0]);
    expect(catalog.backups[0].archive_name).toMatch(/^2025-01-02_03-04-05_[0-9a-f]{12}\.7z$/);
    const retained = join(
      device.archiveRoot,
      '.rgsm-upgrade',
      'originals',
      Buffer.from(GAME_NAME).toString('hex')
    );
    expect((await readdir(retained)).length).toBe(3);
    await applySnapshotViaApi(host, GAME_NAME, ids[0]!);
    expect(await readFile(profile, 'utf8')).toBe('historical-progress');
    expect(await readFile(preferences, 'utf8')).toBe('live-preferences');
    await applySnapshotViaApi(host, GAME_NAME, 'association');
    expect(await readFile(preferences, 'utf8')).toBe('historical-progress');

    await panel.getByRole('button', { name: 'Clean up originals', exact: true }).click();
    await page
      .getByRole('dialog', { name: 'Clean up originals', exact: true })
      .getByRole('button', { name: 'Clean up originals', exact: true })
      .click();
    await expect(panel).toHaveCount(0);
    expect(await readdir(retained)).toEqual([]);
    expect(await readFile(replacement)).toEqual(replacementBytes);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
