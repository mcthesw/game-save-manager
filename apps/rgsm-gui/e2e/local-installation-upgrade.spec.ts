import { test, expect } from '@playwright/test';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import { createRunRoot, hostPost } from './support/rgsm-instance';
import { createSnapshotForGame, getLocalGame } from './support/local-gui';
import { DEVICE_A_ID } from './support/constants';

test('old installations become game settings and ambiguous locations require player selection', async ({
  browser,
}, testInfo) => {
  const runRoot = await createRunRoot('local-installation-upgrade');
  const paths = [
    'known',
    'first installation',
    'second installation with a long folder name',
    'unassigned',
  ].map((name) => join(runRoot, name).replaceAll('\\', '/'));
  for (const path of paths) await writeSaveText(`${path}/profile.sav`, path);
  const names = ['Known game', 'Game with two old installations', 'Manually added game'];
  const device = await seedLocalConfig(runRoot, {
    games: names.map((name) => ({
      name,
      units: [{ type: 'File', path: `${paths[0]}/profile.sav` }],
    })),
  });
  const configPath = join(device.appDataDir, 'GameSaveManager.config.json');
  const config = JSON.parse(await readFile(configPath, 'utf8'));
  config.devices[DEVICE_A_ID].resources = paths.map((path, id) => ({
    id,
    source: 'manual',
    kind: {
      type: 'gameInstallation',
      root_id: 99,
      store: 'other',
      install_dir: 'game',
      path,
    },
  }));
  config.devices[DEVICE_A_ID].next_resource_id = 4;
  for (const game of config.games)
    game.save_paths[0].source = {
      type: 'manifestPattern',
      pattern: '<base>/profile.sav',
      unit_type: 'File',
    };
  config.games[0].device_bindings = { [DEVICE_A_ID]: { installationIds: [0] } };
  config.games[1].device_bindings = { [DEVICE_A_ID]: { installationIds: [1, 2] } };
  await writeFile(configPath, JSON.stringify(config));
  const session = await startLocalSession(browser, {
    runRoot,
    device,
    label: 'installation-upgrade',
  });
  let failed = false;
  try {
    const { page, host } = session;
    expect((await getLocalGame(host, names[0]!)).device_bindings).toMatchObject({
      [DEVICE_A_ID]: { installationPath: paths[0] },
    });
    await createSnapshotForGame(host, names[0]!, 'automatic migration');
    const unresolved = await getLocalGame(host, names[1]!);
    const blocked = await hostPost(host, '/api/v1/create-snapshot', {
      game: unresolved,
      describe: 'must not choose one',
    });
    expect(blocked.ok).toBe(false);

    await page.goto('/Settings');
    await page.getByRole('button', { name: 'Auto Scan', exact: true }).click();
    const panel = page.getByTestId('legacy-installations');
    await expect(panel.getByTestId('legacy-installation')).toHaveCount(3);
    const second = panel.getByTestId('legacy-installation').filter({ hasText: paths[2] });
    await second.scrollIntoViewIfNeeded();
    await page.setViewportSize({ width: 800, height: 850 });
    await page.screenshot({
      path: testInfo.outputPath('legacy-installations-narrow.png'),
      animations: 'disabled',
    });
    await second.getByRole('button', { name: 'Use for this game', exact: true }).click();
    await expect
      .poll(async () => (await getLocalGame(host, names[1]!)).device_bindings)
      .toMatchObject({ [DEVICE_A_ID]: { installationPath: paths[2] } });
    await expect(panel.getByTestId('legacy-installation')).toHaveCount(1);
    const orphan = panel.getByTestId('legacy-installation');
    await orphan.getByRole('combobox', { name: 'Choose game' }).click();
    await page.getByRole('option', { name: names[2], exact: true }).click();
    await orphan.getByRole('button', { name: 'Use for this game', exact: true }).click();
    await expect(panel).toHaveCount(0);
    await createSnapshotForGame(host, names[1]!, 'chosen installation');
    await createSnapshotForGame(host, names[2]!, 'assigned installation');
    for (const path of paths) expect(await readFile(`${path}/profile.sav`, 'utf8')).toBe(path);
    const saved = await hostPost<{ devices: Record<string, { resources: unknown[] }> }>(
      host,
      '/api/v1/get-local-config'
    );
    expect(saved.data.devices[DEVICE_A_ID]?.resources).toEqual([]);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
