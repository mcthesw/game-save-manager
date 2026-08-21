import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { DEVICE_A_ID } from './support/constants';
import {
  cloudPaths,
  expectDeviceHead,
  expectDeviceProfiles,
  expectLiveParentChildGraph,
  expectLocalGeneration,
  expectNamespaceDescriptor,
  expectNoCutoverProgress,
  expectNoNamespace,
  expectPersistedCutoverProgress,
  expectSharedLibraryHasGame,
  expectV1ObjectsUnchanged,
  expectVerifiedArchives,
  readJson,
} from './support/cloud-assertions';
import { seedLegacyV1Scene } from './support/cloud-fixture';
import {
  confirmCutover,
  expectCutoverError,
  expectCutoverSuccess,
  expectLegacyUploadBlocked,
  expectLibraryKind,
  getGeneration,
  inspectLibrary,
  openApp,
  openSyncSettings,
} from './support/gui';
import {
  createRunRoot,
  hostPost,
  newDeviceContext,
  removeRunRoot,
  startRgsmHost,
  startTestWeb,
  type RgsmHost,
} from './support/rgsm-instance';

test('V1 to V2 cutover interrupts, resumes, and stays idempotent', async ({ browser }) => {
  const runRoot = await createRunRoot('cutover');
  const seeded = await seedLegacyV1Scene(runRoot);
  const vite = await startTestWeb();
  const v1Bytes = {
    config: seeded.v1ConfigBytes,
    backups: seeded.v1BackupsBytes,
    parent: seeded.parentArchiveBytes,
    child: seeded.childArchiveBytes,
  };

  let host: RgsmHost | undefined;
  let context;
  let failed = false;
  try {
    host = await startRgsmHost({
      appDataDir: seeded.deviceA.appDataDir,
      deviceId: DEVICE_A_ID,
      logPath: join(runRoot, 'logs', 'host-a.log'),
      env: { RGSM_E2E_CUTOVER_INTERRUPT_AFTER_ARCHIVES: '1' },
    });
    const started = await newDeviceContext(browser, host);
    context = started.context;
    const page = started.page;
    await openApp(page);
    await openSyncSettings(page);
    await expectLibraryKind(page, 'cutover');
    await confirmCutover(page);
    await expectCutoverError(page);
    expect((await hostPost(host, '/api/v1/get-build-info')).ok).toBe(true);
    await expectPersistedCutoverProgress(seeded.deviceA.appDataDir, 1);
    expectNoNamespace(seeded.cloudRoot);
    await expectV1ObjectsUnchanged(seeded.cloudRoot, v1Bytes);

    await host.stop();
    await context.close();

    host = await startRgsmHost({
      appDataDir: seeded.deviceA.appDataDir,
      deviceId: DEVICE_A_ID,
      logPath: join(runRoot, 'logs', 'host-a-resume.log'),
    });
    const resumed = await newDeviceContext(browser, host);
    context = resumed.context;
    await openApp(resumed.page);
    await openSyncSettings(resumed.page);
    const inspect = await hostPost<{
      kind?: string;
      message?: string;
      resumable?: boolean;
    }>(host, '/api/v1/inspect-cloud-library');
    expect(inspect.ok, inspect.raw).toBe(true);
    expect(inspect.data.kind).toBe('cutover_required');
    expect(inspect.data.resumable).toBe(true);
    await expectLibraryKind(resumed.page, 'resume');
    await confirmCutover(resumed.page, true);
    await expectCutoverSuccess(resumed.page);

    expect(await getGeneration(host)).toBe('v2');
    await expectLocalGeneration(seeded.deviceA.appDataDir, 'v2');
    const paths = cloudPaths(seeded.cloudRoot);
    expectNamespaceDescriptor(await readJson(paths.namespace));
    expectSharedLibraryHasGame(await readJson(paths.sharedLibrary));
    const manifest = await readJson(paths.manifest);
    expectLiveParentChildGraph(manifest);
    expectDeviceHead(manifest, DEVICE_A_ID, '2026-01-02_12-00-00');
    await expectDeviceProfiles(seeded.cloudRoot);
    await expectVerifiedArchives(seeded.cloudRoot, {
      parent: seeded.parentArchiveBytes,
      child: seeded.childArchiveBytes,
    });
    await expectV1ObjectsUnchanged(seeded.cloudRoot, v1Bytes);

    await expectLegacyUploadBlocked(host, seeded.cloudRoot);
    const afterLegacy = await readJson(paths.manifest);
    expect(afterLegacy.revision).toBe(manifest.revision);
    expect(
      Object.keys((afterLegacy.games as Record<string, unknown>)['Echo Keep'] as object)
    ).toEqual(Object.keys((manifest.games as Record<string, unknown>)['Echo Keep'] as object));

    await host.stop();
    await context.close();
    host = await startRgsmHost({
      appDataDir: seeded.deviceA.appDataDir,
      deviceId: DEVICE_A_ID,
      logPath: join(runRoot, 'logs', 'host-a-active.log'),
    });
    const active = await newDeviceContext(browser, host);
    context = active.context;
    await openApp(active.page);
    await openSyncSettings(active.page);
    expect((await inspectLibrary(host)).data.kind).toBe('active');
    await expectLibraryKind(active.page, 'active');
    expectNoCutoverProgress(seeded.deviceA.appDataDir);
    expect(
      existsSync(join(seeded.deviceA.appDataDir, 'GameSaveManager.cloud-cutover.staging'))
    ).toBe(false);
    const again = await readJson(paths.manifest);
    expect(again.revision).toBe(manifest.revision);
    await expectVerifiedArchives(seeded.cloudRoot, {
      parent: seeded.parentArchiveBytes,
      child: seeded.childArchiveBytes,
    });
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await context?.close();
    await host?.stop();
    await vite.stop();
    if (!failed) {
      await removeRunRoot(runRoot);
    }
  }
});
