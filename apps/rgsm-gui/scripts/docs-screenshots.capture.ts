import { expect, test, type Page } from '@playwright/test';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { DEVICE_A_ID, DEVICE_B_ID } from '../e2e/support/constants';
import { seedEmptyCloudWithLocalGame } from '../e2e/support/cloud-fixture';
import {
  changeGameMode,
  createLibrary,
  createPublishedSnapshot,
  openApp,
} from '../e2e/support/gui';
import { seedLocalConfig, writeSaveText } from '../e2e/support/local-fixture';
import { listSnapshotsFor } from '../e2e/support/local-gui';
import { startLocalSession } from '../e2e/support/local-session';
import {
  createRunRoot,
  newDeviceContext,
  startRgsmHost,
  workspacePath,
} from '../e2e/support/rgsm-instance';
import { closeResources } from '../e2e/support/process';

const output = workspacePath('apps', 'rgsm-docs', 'static', 'img', 'guide');
const gameName = '星露谷物语';

async function capture(page: Page, name: string) {
  const dismiss = page.getByRole('button', { name: '全部清除', exact: true });
  if (await dismiss.isVisible()) await dismiss.click();
  await expect(page.locator('.activity-panel')).toBeHidden();
  await page.evaluate(() => document.fonts.ready);
  // Focused controls can scroll the overflow-hidden shell during setup.
  await page.locator('.app-shell').evaluate((element) => element.scrollTo(0, 0));
  await expect
    .poll(() =>
      page.locator('.sidebar-search').evaluate((element) => element.getBoundingClientRect().top)
    )
    .toBe(0);
  await page.screenshot({ path: join(output, name), animations: 'disabled' });
}

async function showChineseSync(page: Page) {
  const settings = page.getByRole('button', { name: 'Settings', exact: true });
  if (await settings.isVisible()) {
    await settings.click();
    await page.getByRole('combobox', { name: 'Choose language' }).click();
    await page.getByRole('option', { name: /zh_SIMPLIFIED/ }).click();
  }
  await page.getByRole('button', { name: '同步', exact: true }).click();
  await expect(page.getByRole('button', { name: '同步方式', exact: true })).toBeVisible();
}

// This opt-in capture uses the real host with disposable sample saves. It is not
// part of the regression suite and never opens the player's application data.
test('capture the Chinese 1.9 user guide', async ({ browser }) => {
  const runRoot = await createRunRoot('docs');
  const savePath = join(runRoot, 'saves', 'farm.txt');
  const device = await seedLocalConfig(runRoot, {
    games: ['星露谷物语', '艾尔登法环', '空洞骑士'].map((name) => ({
      name,
      units: [{ type: 'File', path: savePath }],
    })),
    settings: { locale: 'zh_SIMPLIFIED' },
  });
  const configPath = join(device.appDataDir, 'GameSaveManager.config.json');
  const config = JSON.parse(await readFile(configPath, 'utf8'));
  config.devices[DEVICE_A_ID].name = '我的电脑';
  await writeFile(configPath, JSON.stringify(config));
  await mkdir(output, { recursive: true });
  await writeSaveText(savePath, 'spring day 1');
  const session = await startLocalSession(browser, { runRoot, device, label: 'docs' });
  let failed = false;
  try {
    const { page, host } = session;
    await page.setViewportSize({ width: 1280, height: 800 });
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Emulation.setDeviceMetricsOverride', {
      width: 1280,
      height: 800,
      deviceScaleFactor: 2,
      mobile: false,
    });
    await page.goto(`/Management/${encodeURIComponent(gameName)}`);
    await expect(page.getByRole('button', { name: '创建新快照', exact: true })).toBeVisible();
    for (const [index, description] of [
      '春季第 1 天 · 初到农场',
      '春季第 8 天 · 修好桥梁',
      '春季第 13 天 · 参加复活节',
      '夏季第 1 天 · 新的开始',
    ].entries()) {
      await writeSaveText(savePath, `sample progress ${index}`);
      await page.getByPlaceholder('请输入新存档描述信息').fill(description);
      await page.getByRole('button', { name: '创建新快照', exact: true }).click();
      await expect
        .poll(async () => (await listSnapshotsFor(host, gameName)).length)
        .toBe(index + 1);
    }
    await capture(page, 'snapshots.png');
    await page.getByRole('button', { name: '更多操作', exact: true }).click();
    await page.getByRole('menuitem', { name: '自动保存设置' }).click();
    const automatic = page.getByRole('dialog', { name: '自动保存设置' });
    await automatic.getByRole('switch').first().click();
    await automatic.getByRole('switch').last().click();
    await capture(page, 'automatic.png');
    await automatic.getByRole('button', { name: '取消', exact: true }).click();
    await page.getByRole('button', { name: '设置', exact: true }).click();
    await page.getByRole('button', { name: '界面与外观', exact: true }).click();
    await expect(page.getByRole('combobox', { name: '存档时间显示' })).toBeVisible();
    await capture(page, 'appearance.png');
    await page.getByRole('button', { name: '备份设置', exact: true }).click();
    await expect(page.getByRole('combobox', { name: '备份压缩级别' })).toBeVisible();
    await capture(page, 'backup-settings.png');
    await page.goto('/');
    await page.getByRole('button', { name: '添加游戏', exact: true }).first().click();
    await expect(page.getByRole('dialog', { name: '添加游戏', exact: true })).toBeVisible();
    await capture(page, 'add-game.png');
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});

test('capture the connected cloud library', async ({ browser }) => {
  const runRoot = await createRunRoot('docs-cloud');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  for (const device of [seeded.deviceA, seeded.deviceB]) {
    const configPath = join(device.appDataDir, 'GameSaveManager.config.json');
    const config = JSON.parse(await readFile(configPath, 'utf8'));
    config.devices[DEVICE_A_ID].name = '我的电脑';
    config.devices[DEVICE_B_ID].name = '掌机';
    await writeFile(configPath, JSON.stringify(config));
  }
  const session = await startLocalSession(browser, {
    runRoot,
    device: seeded.deviceA,
    label: 'docs-cloud',
  });
  let failed = false;
  const closers: Array<() => Promise<unknown>> = [() => session.close(failed)];
  try {
    const { page, host } = session;
    await page.setViewportSize({ width: 1280, height: 800 });
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Emulation.setDeviceMetricsOverride', {
      width: 1280,
      height: 800,
      deviceScaleFactor: 2,
      mobile: false,
    });
    await createLibrary(page);
    await createPublishedSnapshot(page, host, '到达回声要塞');
    await changeGameMode(page, 'Cloud Backup');
    await showChineseSync(page);
    await capture(page, 'cloud-library.png');
    const hostB = await startRgsmHost({
      appDataDir: seeded.deviceB.appDataDir,
      deviceId: DEVICE_B_ID,
      logPath: join(runRoot, 'logs', 'docs-handheld.log'),
    });
    closers.push(hostB.stop);
    const deviceB = await newDeviceContext(browser, hostB);
    closers.push(() => deviceB.context.close());
    await openApp(deviceB.page);
    await createPublishedSnapshot(deviceB.page, hostB, '击败守门人');
    await page.bringToFront();
    await page.reload();
    await expect(page.getByRole('button', { name: /^(Settings|设置)$/ })).toBeVisible();
    await showChineseSync(page);
    await page
      .getByRole('button', { name: /^(比较进度|进度已分叉，点此比较)$/ })
      .first()
      .click({ timeout: 20_000 });
    await expect(page.getByRole('dialog', { name: '比较 Echo Keep 的进度' })).toBeVisible();
    await expect(page.getByRole('button', { name: '使用此进度', exact: true })).toBeVisible();
    await capture(page, 'progress.png');
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await closeResources(closers);
  }
});
