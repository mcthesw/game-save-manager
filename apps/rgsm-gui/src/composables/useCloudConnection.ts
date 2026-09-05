import { commands } from '~/api/commands';
import { useConfig } from './useConfig';

let connecting: { key: string; promise: ReturnType<typeof commands.connectCloudLibrary> } | null =
  null;

/** Startup and saved-connection UI share the same in-flight connection. */
export async function connectSavedCloudLibrary() {
  const key = JSON.stringify(useConfig().config.value.settings.cloud_settings);
  if (!connecting || connecting.key !== key) {
    const promise = commands.connectCloudLibrary().finally(() => {
      if (connecting?.promise === promise) connecting = null;
    });
    connecting = { key, promise };
  }
  const result = await connecting.promise;
  if (result.status === 'ok' && result.data.kind === 'active') {
    await useConfig().refreshConfig();
  }
  return result;
}
