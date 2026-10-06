import { test, expect, type Locator, type Page } from '@playwright/test';
import { seedLocalConfig } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import { createRunRoot } from './support/rgsm-instance';
import { openGame } from './support/gui';
import { GAME_NAME } from './support/constants';
import { join } from 'node:path';

async function checkEditor(page: Page, editor: Locator, dialog: Locator) {
  await editor.fill('<');
  const suggestions = page.locator('.pvi-suggestions');
  await expect(suggestions).toBeVisible();
  await expect
    .poll(async () => {
      const box = await suggestions.boundingBox();
      const viewport = page.viewportSize()!;
      return !!box && box.y >= 0 && box.y + box.height <= viewport.height;
    })
    .toBe(true);
  const home = suggestions.locator('.pvi-suggestion-item').filter({ hasText: '<home>' });
  // A visible DOM node can still be behind a modal or unable to receive pointer events.
  await home.click();
  await expect(editor).toHaveText('<home>');
  await expect(editor.locator('.pvi-tag')).toHaveText('<home>');
  await expect(dialog).toBeVisible();
  await expect(editor).toBeFocused();

  await editor.fill('<hom');
  await expect(suggestions.locator('.pvi-suggestion-item')).toHaveCount(1);
  await editor.press('Enter');
  await expect(editor).toHaveText('<home>');
  await expect(suggestions).toBeHidden();
  await editor.press('End');
  await editor.press('Backspace');
  await expect(editor).toBeEmpty();

  await editor.fill('<');
  await editor.press('ArrowDown');
  const selected = await suggestions.locator('.active .pvi-suggestion-var').innerText();
  await editor.press('Tab');
  await expect(editor).toHaveText(selected);
  await expect(editor).toBeFocused();

  await editor.fill('<hom');
  await editor.press('Escape');
  await expect(suggestions).toBeHidden();
  await expect(dialog).toBeVisible();
  await editor.fill('<');
  // Advance past the debounce: partial input is not an error while editing.
  await page.waitForTimeout(650);
  const wrapper = editor.locator('xpath=ancestor::*[contains(@class,"pvi-wrapper")]');
  await expect(wrapper.locator('.pvi-status--error')).toHaveCount(0);
  await dialog.getByRole('heading', { level: 2 }).click();
  await expect(suggestions).toBeHidden();
  if (await wrapper.locator('.pvi-status').count()) {
    await expect(wrapper).toContainText('Complete the path variable with >');
  }
  await editor.click();
  await expect(suggestions).toBeVisible();
  await editor.fill('<home>');
  await expect(wrapper.locator('.pvi-status--ok')).toHaveCount(1);
  await editor.fill('<unknownVariable>');
  await expect(wrapper.locator('.pvi-status--error')).toHaveCount(1);
  await editor.fill('<home>');
  await expect(wrapper.locator('.pvi-status--ok')).toHaveCount(1);
  await expect(dialog).toBeVisible();
}

for (const surface of ['add', 'manage', 'import'] as const) {
  test(`path autocomplete works in every ${surface} editor`, async ({ browser }, testInfo) => {
    const runRoot = await createRunRoot(`path-autocomplete-${surface}`);
    const device = await seedLocalConfig(runRoot, {
      games: [
        {
          name: GAME_NAME,
          units: [
            { type: 'File', path: join(runRoot, 'save.txt') },
            { type: 'Folder', path: join(runRoot, 'saves') },
            { type: 'File', path: join(runRoot, 'disabled.txt'), enabled: false },
          ],
        },
      ],
    });
    const session = await startLocalSession(browser, { runRoot, device, label: surface });
    let failed = false;
    try {
      const { page } = session;
      await page.setViewportSize({ width: 1100, height: 650 });
      let dialog: Locator;
      if (surface === 'manage') {
        await openGame(page);
        await page.getByRole('button', { name: 'View managed files' }).click();
        dialog = page.getByRole('dialog');
      } else {
        await page.getByRole('button', { name: 'Add game' }).first().click();
        dialog = page.getByRole('dialog');
        if (surface === 'add') {
          // Add a row through an in-app prompt; native file pickers are outside browser coverage.
          await dialog.getByRole('button', { name: 'Add registry key' }).click();
          const prompt = page.getByRole('dialog', { name: 'Add registry key', exact: true });
          await prompt.getByRole('textbox').fill('HKEY_CURRENT_USER\\Software\\RGSM_TEST');
          await prompt.getByRole('button', { name: 'confirm', exact: true }).click();
          await expect(prompt).toBeHidden();
        } else {
          await dialog.getByRole('button', { name: 'Detect local games' }).click();
          const imports = page.getByRole('dialog', {
            name: 'Import Games (Auto-detect Save Locations)',
          });
          await imports
            .getByRole('checkbox', { name: 'Show only locally installed games' })
            .uncheck({ timeout: 120_000 });
          const search = imports.getByRole('textbox', { name: 'Search games...' });
          await expect(search).toBeEnabled({ timeout: 120_000 });
          await search.fill('Stardew Valley');
          await imports.getByRole('checkbox', { name: 'Stardew Valley', exact: true }).check();
          await imports.getByRole('button', { name: /Import 1 selected game/ }).click();
          dialog = page.getByRole('dialog', { name: 'Customize Import: Stardew Valley' });
          await expect(dialog).toBeVisible();
          await expect(
            dialog.getByRole('button', { name: 'Verify paths', exact: true })
          ).toBeEnabled();
        }
      }
      const editors = dialog.locator('.pvi-editor');
      expect(await editors.count()).toBeGreaterThan(0);
      for (const editor of await editors.all()) await checkEditor(page, editor, dialog);
      await editors.first().fill('<');
      await expect(page.locator('.pvi-suggestions')).toBeVisible();
      await page.screenshot({
        path: testInfo.outputPath(`acceptance-path-autocomplete-${surface}.png`),
      });
    } catch (error) {
      failed = true;
      throw error;
    } finally {
      await session.close(failed);
    }
  });
}
