import { test, expect } from '@playwright/test';
import { GAME_NAME } from './support/constants';
import { openGame } from './support/gui';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { createSnapshotForGame } from './support/local-gui';
import { startLocalSession } from './support/local-session';
import { createRunRoot } from './support/rgsm-instance';

for (const width of [1280, 420]) {
  test(`confirmation dialog stays centered throughout its entrance animation at ${width}px`, async ({
    browser,
  }) => {
    const runRoot = await createRunRoot('dialog-animation');
    const device = await seedLocalConfig(runRoot, {
      settings: { confirm_before_apply_latest: true },
    });
    await writeSaveText(device.savePath, 'dialog animation save\n');
    const session = await startLocalSession(browser, {
      runRoot,
      device,
      label: 'dialog-animation',
    });
    let failed = false;
    try {
      await session.page.setViewportSize({ width, height: 720 });
      await createSnapshotForGame(session.host, GAME_NAME, 'Animation test');
      await openGame(session.page);
      const pause = await session.page.addStyleTag({
        content: '.k-dialog { animation-play-state: paused !important; }',
      });
      await session.page.getByRole('button', { name: 'Apply latest' }).click();
      const dialog = session.page.getByRole('dialog', { name: 'Warning' });
      for (const time of [0, 75, 150]) {
        const center = await dialog.evaluate((element, currentTime) => {
          const animation = element.getAnimations()[0];
          if (!animation) throw new Error('Missing dialog entrance animation');
          animation.currentTime = currentTime;
          const rect = element.getBoundingClientRect();
          return {
            x: rect.left + rect.width / 2 - window.innerWidth / 2,
            y: rect.top + rect.height / 2 - window.innerHeight / 2,
          };
        }, time);
        expect(center.x, `horizontal center at ${time}ms`).toBeCloseTo(0, 0);
        expect(center.y, `vertical center at ${time}ms`).toBeCloseTo(0, 0);
      }
      await pause.evaluate((element) => element.parentNode?.removeChild(element));
      const checkbox = dialog.getByRole('checkbox', { name: "Don't ask again" });
      await checkbox.check();
      await expect(checkbox).toBeChecked();
      await session.page.screenshot({
        path: test.info().outputPath('acceptance-dialog-animation.png'),
      });
      await dialog.getByRole('button', { name: 'Cancel' }).click();
    } catch (error) {
      failed = true;
      throw error;
    } finally {
      await session.close(failed);
    }
  });
}
