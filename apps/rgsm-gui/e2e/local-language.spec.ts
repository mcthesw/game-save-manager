import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import en from '../../../locales/en_US.json' with { type: 'json' };
import ru from '../../../locales/ru.json' with { type: 'json' };
import { seedLocalConfig } from './support/local-fixture';
import { getSettings } from './support/local-gui';
import { startLocalSession } from './support/local-session';
import { createRunRoot } from './support/rgsm-instance';

test('Russian can be selected, persisted, and switched back with English fallback', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('local-language');
  const device = await seedLocalConfig(runRoot);
  const session = await startLocalSession(browser, { runRoot, device, label: 'local-language' });
  const { page, host } = session;
  let failed = false;
  try {
    await page.goto('/Settings');
    await page.getByRole('combobox', { name: en.home.choose_language }).click();
    await page.getByRole('option', { name: 'Русский - ru', exact: true }).click();
    await expect(
      page.getByRole('heading', { name: ru.settings.general, exact: true })
    ).toBeVisible();
    await expect.poll(async () => (await getSettings(host)).locale).toBe('ru');
    await expect
      .poll(async () => {
        const config = JSON.parse(
          await readFile(
            join(device.appDataDir, 'GameSaveManager.config.v2', 'local-state.json'),
            'utf8'
          )
        );
        return config.interface.locale;
      })
      .toBe('ru');

    await page.reload();
    const language = page.getByRole('combobox', { name: ru.home.choose_language });
    await expect(language).toHaveText('Русский - ru');
    await expect(
      page.getByRole('heading', { name: ru.settings.general, exact: true })
    ).toBeVisible();

    await page.goto('/About');
    await page.setViewportSize({ width: 800, height: 720 });
    const credit = page.getByText('Andrei Stepanov', { exact: true });
    await credit.scrollIntoViewIfNeeded();
    await expect(credit).toBeInViewport();
    expect(await credit.evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(
      true
    );
    await page.goto('/Settings');

    // Missing translations should still resolve through the application's English fallback.
    const fallback = await page.evaluate(async () => {
      const modulePath = '/src/i18n.ts';
      const { i18n, $t } = await import(modulePath);
      const original = i18n.global.getLocaleMessage('ru');
      const missing = JSON.parse(JSON.stringify(original));
      delete missing.settings.general;
      try {
        i18n.global.setLocaleMessage('ru', missing);
        return $t('settings.general');
      } finally {
        i18n.global.setLocaleMessage('ru', original);
      }
    });
    expect(fallback).toBe(en.settings.general);

    await language.click();
    await page.getByRole('option', { name: 'English - en_US', exact: true }).click();
    await expect.poll(async () => (await getSettings(host)).locale).toBe('en_US');
    await expect(
      page.getByRole('heading', { name: en.settings.general, exact: true })
    ).toBeVisible();
    await page.reload();
    await expect(page.getByRole('combobox', { name: en.home.choose_language })).toHaveText(
      'English - en_US'
    );
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
