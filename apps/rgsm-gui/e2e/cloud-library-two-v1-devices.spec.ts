import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { DEVICE_A_ID, DEVICE_B_ID } from './support/constants';
import {
  cloudPaths,
  expectDeviceHead,
  expectDeviceProfiles,
  expectFinalTombstone,
  expectLiveParentChildGraph,
  expectLocalGeneration,
  expectNamespaceDescriptor,
  expectNoDeviceHead,
  expectSharedLibraryHasGame,
  expectV1ObjectsUnchanged,
  localArchivePath,
  readDeviceProfile,
  readJson,
  snapshotNode,
} from './support/cloud-assertions';
import { readSave, seedLegacyV1Scene, writeSave } from './support/cloud-fixture';
import {
  acceptRemoteProgress,
  applySnapshot,
  changeGameMode,
  confirmCutover,
  confirmJoinKeepCloud,
  deleteCurrentHead,
  downloadSnapshot,
  expectCutoverSuccess,
  expectLegacyUploadBlocked,
  expectLibraryKind,
  getArchiveLibrary,
  getGeneration,
  inspectLibrary,
  openApp,
  openGame,
  openProgressReview,
  openSyncSettings,
  reviewProgress,
  toggleCloudEnabled,
} from './support/gui';
import {
  createRunRoot,
  fsSession,
  hostPost,
  newDeviceContext,
  removeRunRoot,
  startRgsmHost,
  startTestWeb,
  type RgsmHost,
} from './support/rgsm-instance';

test('two V1 devices cut over, join, and keep V2 device boundaries', async ({ browser }) => {
  const runRoot = await createRunRoot('two-devices');
  const seeded = await seedLegacyV1Scene(runRoot);
  const vite = await startTestWeb();
  const v1Bytes = {
    config: seeded.v1ConfigBytes,
    backups: seeded.v1BackupsBytes,
    parent: seeded.parentArchiveBytes,
    child: seeded.childArchiveBytes,
  };

  let hostA: RgsmHost | undefined;
  let hostB: RgsmHost | undefined;
  let contextA;
  let contextB;
  let failed = false;
  try {
    hostA = await startRgsmHost({
      appDataDir: seeded.deviceA.appDataDir,
      deviceId: DEVICE_A_ID,
      logPath: join(runRoot, 'logs', 'host-a.log'),
    });
    hostB = await startRgsmHost({
      appDataDir: seeded.deviceB.appDataDir,
      deviceId: DEVICE_B_ID,
      logPath: join(runRoot, 'logs', 'host-b.log'),
    });
    const startedA = await newDeviceContext(browser, hostA);
    const startedB = await newDeviceContext(browser, hostB);
    contextA = startedA.context;
    contextB = startedB.context;
    const pageA = startedA.page;
    const pageB = startedB.page;

    await openApp(pageA);
    await openApp(pageB);
    await openSyncSettings(pageA);
    await openSyncSettings(pageB);
    await expectLibraryKind(pageA, 'cutover');
    await expectLibraryKind(pageB, 'cutover');
    expect((await inspectLibrary(hostA)).data.kind).toBe('cutover_required');
    expect((await inspectLibrary(hostB)).data.kind).toBe('cutover_required');
    expect(await getGeneration(hostA)).toBe('legacy_v1');
    expect(await getGeneration(hostB)).toBe('legacy_v1');
    expectNoV2(seeded.cloudRoot);

    await writeSave(seeded.deviceA, 'v1-exchange-from-a\n');
    const upload = await hostPost(hostA, '/api/v1/cloud-upload-all', {
      session: fsSession(seeded.cloudRoot),
    });
    expect(upload.ok, upload.raw).toBe(true);
    const download = await hostPost(hostB, '/api/v1/cloud-download-all', {
      session: fsSession(seeded.cloudRoot),
    });
    expect(download.ok, download.raw).toBe(true);
    expect(existsSync(cloudPaths(seeded.cloudRoot).namespace)).toBe(false);

    await confirmCutover(pageA);
    await expectCutoverSuccess(pageA);
    expect(await getGeneration(hostA)).toBe('v2');
    expect(await getGeneration(hostB)).toBe('legacy_v1');
    await expectLocalGeneration(seeded.deviceA.appDataDir, 'v2');
    await expectLocalGeneration(seeded.deviceB.appDataDir, 'legacy_v1');

    const paths = cloudPaths(seeded.cloudRoot);
    expectNamespaceDescriptor(await readJson(paths.namespace));
    expectSharedLibraryHasGame(await readJson(paths.sharedLibrary));
    await expectDeviceProfiles(seeded.cloudRoot);
    const afterCutover = await readJson(paths.manifest);
    expectLiveParentChildGraph(afterCutover);
    await expectV1ObjectsUnchanged(seeded.cloudRoot, {
      ...v1Bytes,
      config: await readFile(paths.v1Config),
      backups: await readFile(paths.v1Backups),
    });

    await pageB.reload();
    await openSyncSettings(pageB);
    expect((await inspectLibrary(hostB)).data.kind).toBe('join_required');
    await expectLibraryKind(pageB, 'join');

    await confirmJoinKeepCloud(pageB);
    expect(await getGeneration(hostA)).toBe('v2');
    expect(await getGeneration(hostB)).toBe('v2');
    await expectDeviceProfiles(seeded.cloudRoot);
    await expectLegacyUploadBlocked(hostA, seeded.cloudRoot);
    await expectLegacyUploadBlocked(hostB, seeded.cloudRoot);

    await writeSave(seeded.deviceA, 'a-forward-save\n');
    const aForward = await createAndUploadSnapshot(hostA, 'A forward');
    const afterForward = await readJson(paths.manifest);
    expectDeviceHead(afterForward, DEVICE_A_ID, aForward);
    expect((snapshotNode(afterForward, aForward).state as { type?: string }).type).toBe('live');

    const saveBeforeApply = await readSave(seeded.deviceB);
    await openGame(pageB);
    await downloadSnapshot(pageB, aForward);
    expect(existsSync(localArchivePath(seeded.deviceB.appDataDir, aForward))).toBe(true);
    expect(await readSave(seeded.deviceB)).toBe(saveBeforeApply);
    await applySnapshot(pageB, aForward);
    expect(await readSave(seeded.deviceB)).toBe('a-forward-save\n');
    expectDeviceHead(await readJson(paths.manifest), DEVICE_A_ID, aForward);
    await openSyncSettings(pageA);
    await changeGameMode(pageA, 'Manual');
    const profileA = await readDeviceProfile(seeded.cloudRoot, DEVICE_A_ID);
    const profileB = await readDeviceProfile(seeded.cloudRoot, DEVICE_B_ID);
    const gameA = (
      profileA.games as Record<string, { sync_mode?: string; cloud_sync_enabled?: boolean }>
    )['Echo Keep'];
    const gameB = (
      profileB.games as Record<string, { sync_mode?: string; cloud_sync_enabled?: boolean }>
    )['Echo Keep'];
    expect(gameA.sync_mode).toBe('manual');
    expect(gameB.sync_mode).not.toBe('manual');
    await toggleCloudEnabled(pageB, hostB, false);
    const profileAAfter = await readDeviceProfile(seeded.cloudRoot, DEVICE_A_ID);
    expect(
      (profileAAfter.games as Record<string, { sync_mode?: string }>)['Echo Keep'].sync_mode
    ).toBe('manual');

    await writeSave(seeded.deviceA, 'branch-from-a\n');
    await writeSave(seeded.deviceB, 'branch-from-b\n');
    const aBranch = await createAndUploadSnapshot(hostA, 'A branch');
    const bBranch = await createAndUploadSnapshot(hostB, 'B branch');
    const diverged = await readJson(paths.manifest);
    expectDeviceHead(diverged, DEVICE_A_ID, aBranch);
    expectDeviceHead(diverged, DEVICE_B_ID, bBranch);
    expect(snapshotNode(diverged, aBranch)).toBeTruthy();
    expect(snapshotNode(diverged, bBranch)).toBeTruthy();

    const review = await reviewProgress(hostA);
    expect(review.requires_choice).toBe(true);
    await openProgressReview(pageA);
    await acceptRemoteProgress(pageA, bBranch);
    const afterAccept = await readJson(paths.manifest);
    expectDeviceHead(afterAccept, DEVICE_A_ID, bBranch);
    expectDeviceHead(afterAccept, DEVICE_B_ID, bBranch);
    expect(await readSave(seeded.deviceA)).toBe('branch-from-b\n');

    await openGame(pageA);
    await deleteCurrentHead(pageA, bBranch);
    const afterDelete = await readJson(paths.manifest);
    expectFinalTombstone(afterDelete, bBranch);
    expectNoDeviceHead(afterDelete, DEVICE_A_ID, bBranch);
    expect(existsSync(localArchivePath(seeded.deviceA.appDataDir, bBranch))).toBe(false);
    expect(
      existsSync(join(seeded.cloudRoot, 'v2', 'archives', 'Echo Keep', `${bBranch}.zip`))
    ).toBe(false);

    await hostB.stop();
    hostB = await startRgsmHost({
      appDataDir: seeded.deviceB.appDataDir,
      deviceId: DEVICE_B_ID,
      logPath: join(runRoot, 'logs', 'host-b-reconcile.log'),
    });
    await contextB.close();
    const restartedB = await newDeviceContext(browser, hostB);
    contextB = restartedB.context;
    await openApp(restartedB.page);
    await openSyncSettings(restartedB.page);
    await getArchiveLibrary(hostB);
    expect(existsSync(localArchivePath(seeded.deviceB.appDataDir, bBranch))).toBe(false);

    const saveA = await readSave(seeded.deviceA);
    const saveB = await readSave(seeded.deviceB);
    const genA = await getGeneration(hostA);
    const genB = await getGeneration(hostB);
    await hostA.stop();
    await hostB.stop();
    hostA = await startRgsmHost({
      appDataDir: seeded.deviceA.appDataDir,
      deviceId: DEVICE_A_ID,
      logPath: join(runRoot, 'logs', 'host-a-restart.log'),
    });
    hostB = await startRgsmHost({
      appDataDir: seeded.deviceB.appDataDir,
      deviceId: DEVICE_B_ID,
      logPath: join(runRoot, 'logs', 'host-b-restart.log'),
    });
    await contextA.close();
    await contextB.close();
    const finalA = await newDeviceContext(browser, hostA);
    const finalB = await newDeviceContext(browser, hostB);
    contextA = finalA.context;
    contextB = finalB.context;
    await openApp(finalA.page);
    await openApp(finalB.page);
    await openSyncSettings(finalA.page);
    await openSyncSettings(finalB.page);
    await expectLibraryKind(finalA.page, 'active');
    await expectLibraryKind(finalB.page, 'active');
    expect(await getGeneration(hostA)).toBe(genA);
    expect(await getGeneration(hostB)).toBe(genB);
    expect(await readSave(seeded.deviceA)).toBe(saveA);
    expect(await readSave(seeded.deviceB)).toBe(saveB);
    expectFinalTombstone(await readJson(paths.manifest), bBranch);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await contextA?.close();
    await contextB?.close();
    await hostA?.stop();
    await hostB?.stop();
    await vite.stop();
    if (!failed) {
      await removeRunRoot(runRoot);
    }
  }
});

function expectNoV2(cloudRoot: string): void {
  expect(existsSync(cloudPaths(cloudRoot).namespace)).toBe(false);
}

async function listSnapshots(host: RgsmHost): Promise<Array<{ date: string; describe?: string }>> {
  const config = await hostPost<{ games: Array<{ name: string }> }>(
    host,
    '/api/v1/get-local-config'
  );
  expect(config.ok, config.raw).toBe(true);
  const game = config.data.games.find((item) => item.name === 'Echo Keep');
  const snapshots = await hostPost<{ backups: Array<{ date: string; describe?: string }> }>(
    host,
    '/api/v1/get-game-snapshots-info',
    { game }
  );
  expect(snapshots.ok, snapshots.raw).toBe(true);
  return snapshots.data.backups;
}

async function createAndUploadSnapshot(host: RgsmHost, describe: string): Promise<string> {
  const config = await hostPost<{ games: Array<Record<string, unknown>> }>(
    host,
    '/api/v1/get-local-config'
  );
  expect(config.ok, config.raw).toBe(true);
  const game = config.data.games.find((item) => item.name === 'Echo Keep');
  const created = await hostPost(host, '/api/v1/create-snapshot', { game, describe });
  expect(created.ok, created.raw).toBe(true);
  const snapshots = await listSnapshots(host);
  const snapshot = snapshots.find((item) => item.describe === describe);
  expect(snapshot, `missing snapshot ${describe}`).toBeTruthy();
  const uploaded = await hostPost(host, '/api/v1/upload-cloud-archive', {
    gameId: 'Echo Keep',
    snapshotId: snapshot!.date,
  });
  expect(uploaded.ok, uploaded.raw).toBe(true);
  return snapshot!.date;
}
