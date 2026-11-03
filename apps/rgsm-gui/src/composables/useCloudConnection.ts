import { computed, shallowRef } from 'vue';
import { commands, type CloudLibraryStatus } from '~/api/commands';
import { useConfig } from './useConfig';
import { clearCloudLibrary, refreshCloudLibrary } from './useCloudLibrary';

const settingsKey = () => JSON.stringify(useConfig().config.value.settings.cloud_settings);
const lastStatus = shallowRef<{ key: string; status: CloudLibraryStatus } | null>(null);

/** What the last connection check found, so opening the page does not repeat it. */
export const rememberedCloudLibraryStatus = computed(() =>
  lastStatus.value?.key === settingsKey() ? lastStatus.value.status : null
);

export function rememberCloudLibraryStatus(status: CloudLibraryStatus | null) {
  lastStatus.value = status ? { key: settingsKey(), status } : null;
}

/** An active (V2) library is already bound; the background worker keeps it current. */
export async function isCloudLibraryUnconnected() {
  const generation = await commands.getCloudNamespaceGeneration();
  return generation.status === 'error' || generation.data !== 'v2';
}

let connecting: { key: string; promise: ReturnType<typeof commands.connectCloudLibrary> } | null =
  null;

/** Startup and saved-connection UI share the same in-flight connection. */
export async function connectSavedCloudLibrary() {
  const key = JSON.stringify(useConfig().config.value.settings.cloud_settings);
  if (!connecting || connecting.key !== key) {
    const promise = commands
      .connectCloudLibrary()
      .then(async (result) => {
        if (result.status === 'ok') rememberCloudLibraryStatus(result.data);
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
