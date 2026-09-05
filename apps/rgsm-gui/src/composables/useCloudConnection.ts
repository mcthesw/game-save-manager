import { commands } from '~/api/commands';
import { useConfig } from './useConfig';
import { clearCloudLibrary, refreshCloudLibrary } from './useCloudLibrary';

let connecting: { key: string; promise: ReturnType<typeof commands.connectCloudLibrary> } | null =
  null;

/** Startup and saved-connection UI share the same in-flight connection. */
export async function connectSavedCloudLibrary() {
  const key = JSON.stringify(useConfig().config.value.settings.cloud_settings);
  if (!connecting || connecting.key !== key) {
    const promise = commands
      .connectCloudLibrary()
      .then(async (result) => {
        if (result.status === 'ok' && result.data.kind === 'active') {
          await useConfig().refreshConfig();
          clearCloudLibrary();
          await refreshCloudLibrary();
        }
        return result;
      })
      .finally(() => {
        if (connecting?.promise === promise) connecting = null;
      });
    connecting = { key, promise };
  }
  return connecting.promise;
}
