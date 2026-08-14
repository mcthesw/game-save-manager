import { client } from './generated/client.gen';

type RgsmRuntimeConfig = {
  apiBaseUrl: string;
  token: string;
};

declare global {
  interface Window {
    __RGSM_RUNTIME__?: RgsmRuntimeConfig;
  }
}

const runtime = typeof window === 'undefined' ? undefined : window.__RGSM_RUNTIME__;
const apiBaseUrl = runtime?.apiBaseUrl ?? '';
let authorization = runtime ? `Bearer ${runtime.token}` : undefined;
if (typeof window !== 'undefined') {
  delete window.__RGSM_RUNTIME__;
}

client.setConfig({
  baseUrl: apiBaseUrl,
  headers: authorization ? { Authorization: authorization } : undefined,
});

export function updateAuthorizationToken(token: string) {
  authorization = `Bearer ${token}`;
  client.setConfig({ headers: { Authorization: authorization } });
}

export function apiFetch(path: string, init?: RequestInit) {
  const headers = new Headers(init?.headers);
  if (authorization) headers.set('Authorization', authorization);
  return fetch(`${apiBaseUrl}${path}`, { ...init, headers });
}

export { client };
