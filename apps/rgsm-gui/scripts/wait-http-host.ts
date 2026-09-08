import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

/** Read only the specified profile and verify its authenticated HTTP endpoint. */
export async function waitForHttpHost(
  appDataDir: string,
  {
    signal,
    timeoutMs = 60_000,
    failure = () => undefined,
  }: { signal?: AbortSignal; timeoutMs?: number; failure?: () => Error | undefined } = {}
): Promise<{ apiBaseUrl: string; token: string; port: number }> {
  const deadline = Date.now() + timeoutMs;
  let lastError: unknown;
  while (Date.now() < deadline) {
    signal?.throwIfAborted();
    const processError = failure();
    if (processError) throw processError;
    try {
      const config = JSON.parse(
        await readFile(join(appDataDir, 'GameSaveManager.host.json'), 'utf8')
      );
      if (
        !Number.isInteger(config.port) ||
        config.port < 1 ||
        config.port > 65535 ||
        typeof config.api_token !== 'string' ||
        !config.api_token
      ) {
        throw new Error('Invalid host discovery file');
      }
      const apiBaseUrl = `http://127.0.0.1:${config.port}`;
      const timeout = AbortSignal.timeout(Math.max(1, Math.min(1_000, deadline - Date.now())));
      const response = await fetch(`${apiBaseUrl}/api/v1/get-build-info`, {
        method: 'POST',
        headers: { Authorization: `Bearer ${config.api_token}` },
        signal: signal ? AbortSignal.any([signal, timeout]) : timeout,
      });
      await response.body?.cancel();
      signal?.throwIfAborted();
      if (response.ok) return { apiBaseUrl, token: config.api_token, port: config.port };
      lastError = new Error(`get-build-info returned HTTP ${response.status}`);
    } catch (error) {
      signal?.throwIfAborted();
      lastError = error;
    }
    await delay(Math.max(0, Math.min(250, deadline - Date.now())), undefined, { signal });
  }
  throw new Error(`Timed out waiting for RGSM HTTP host: ${String(lastError)}`);
}
