import { setTimeout as delay } from 'node:timers/promises';

export async function waitForHttpOk(url: string, timeoutMs: number, label: string): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  let lastError: unknown;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url, {
        signal: AbortSignal.timeout(Math.max(1, Math.min(1_000, deadline - Date.now()))),
      });
      await response.body?.cancel();
      if (response.ok) return;
      lastError = new Error(`${label} returned HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await delay(Math.max(0, Math.min(250, deadline - Date.now())));
  }
  throw new Error(`Timed out waiting for ${label}: ${String(lastError)}`);
}
