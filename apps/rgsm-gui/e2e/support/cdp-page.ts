import type { Browser, Page } from '@playwright/test';
import { setTimeout as delay } from 'node:timers/promises';

export async function waitForCdpPage(
  browser: Pick<Browser, 'contexts' | 'isConnected'>,
  appUrl: string,
  timeoutMs = 10_000
): Promise<Page> {
  const deadline = Date.now() + timeoutMs;
  let attachedPages = 0;
  while (Date.now() < deadline) {
    if (!browser.isConnected()) throw new Error('CDP disconnected before the app page was ready');
    const pages = browser.contexts().flatMap((context) => context.pages());
    attachedPages = pages.length;
    const page = pages.find((candidate) => !candidate.isClosed() && candidate.url() === appUrl);
    if (page) return page;
    await delay(Math.min(50, Math.max(0, deadline - Date.now())));
  }
  throw new Error(
    `Timed out waiting for the app page after CDP connected (${attachedPages} attached pages)`
  );
}
