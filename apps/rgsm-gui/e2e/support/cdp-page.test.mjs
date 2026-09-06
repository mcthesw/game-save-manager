import assert from 'node:assert/strict';
import test from 'node:test';
import { waitForCdpPage } from './cdp-page.ts';

const appUrl = 'http://localhost:5173/';
const page = (url, closed = false) => ({ url: () => url, isClosed: () => closed });

test('CDP connection waits for a target to attach and finish navigation', async () => {
  let reads = 0;
  const target = page(appUrl);
  const browser = {
    isConnected: () => true,
    contexts: () => {
      reads += 1;
      if (reads === 1) return [];
      return [{ pages: () => (reads === 2 ? [page('about:blank')] : [target]) }];
    },
  };
  assert.equal(await waitForCdpPage(browser, appUrl, 500), target);
  assert.equal(reads, 3);
});

test('a stale closed target does not stand in for the recreated page', async () => {
  const target = page(appUrl);
  const browser = {
    isConnected: () => true,
    contexts: () => [{ pages: () => [page(appUrl, true), target] }],
  };
  assert.equal(await waitForCdpPage(browser, appUrl, 500), target);
});

test('disconnection and missing-page timeout remain distinct failures', async () => {
  await assert.rejects(
    waitForCdpPage({ isConnected: () => false, contexts: () => [] }, appUrl, 5),
    /disconnected/
  );
  await assert.rejects(
    waitForCdpPage({ isConnected: () => true, contexts: () => [] }, appUrl, 5),
    /Timed out.*app page/
  );
});
