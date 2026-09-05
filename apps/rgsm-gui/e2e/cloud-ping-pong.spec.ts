import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { DEVICE_A_ID, DEVICE_B_ID, GAME_NAME, STORAGE_KEY } from './support/constants';
import {
  cloudArchivePath,
  cloudPaths,
  localArchivePath,
  readJson,
} from './support/cloud-assertions';
import { readSave, seedEmptyCloudWithLocalGame, writeSave } from './support/cloud-fixture';
import {
  applySnapshot,
  connectLibrary,
  createLibrary,
  createPublishedSnapshot,
  downloadSnapshot,
  enableMode,
  openGame,
} from './support/gui';
import { createRunRoot } from './support/rgsm-instance';
import { startDualSession } from './support/session';

const ROUNDS = 3;

async function expectDeviceHeadEventually(
  cloudRoot: string,
  deviceId: string,
  snapshotId: string
): Promise<void> {
  // In Multi-device Sync the device head advances on the snapshot sync
  // coordinator cycle, not synchronously with the upload; allow for a tick.
  await expect
    .poll(
      async () => {
        const manifest = await readJson(cloudPaths(cloudRoot).manifest);
        const games = manifest.games as Record<string, { device_heads: Record<string, string> }>;
        return games[GAME_NAME].device_heads[deviceId];
      },
      { timeout: 90_000 }
    )
    .toBe(snapshotId);
}

test('repeated upload download round trips stay consistent', async ({ browser }) => {
  const runRoot = await createRunRoot('ping-pong');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'ping-pong' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    const first = await createPublishedSnapshot(session.pageA, session.hostA, 'Round 0 from A');
    await enableMode(session.pageA, session.hostA, 'Multi-device Sync', 'Keep in cloud');
    await connectLibrary(session.pageB);
    await openGame(session.pageB);
    await downloadSnapshot(session.pageB, first);
    await enableMode(session.pageB, session.hostB, 'Multi-device Sync', 'Keep in cloud');

    const published: string[] = [first];
    const creators = new Map([[first, DEVICE_A_ID]]);
    let latestA = first;
    let latestB = first;
    for (let round = 1; round <= ROUNDS; round += 1) {
      const aSave = `round-${round}-from-a\n`;
      await writeSave(seeded.deviceA, aSave);
      const aSnap = await createPublishedSnapshot(session.pageA, session.hostA, `Round ${round} A`);
      published.push(aSnap);
      creators.set(aSnap, DEVICE_A_ID);
      latestA = aSnap;

      await openGame(session.pageB);
      await downloadSnapshot(session.pageB, aSnap);
      expect(existsSync(localArchivePath(seeded.deviceB.appDataDir, aSnap))).toBe(true);
      await applySnapshot(session.pageB, aSnap);
      expect(await readSave(seeded.deviceB)).toBe(aSave);

      const bSave = `round-${round}-from-b\n`;
      await writeSave(seeded.deviceB, bSave);
      const bSnap = await createPublishedSnapshot(session.pageB, session.hostB, `Round ${round} B`);
      published.push(bSnap);
      creators.set(bSnap, DEVICE_B_ID);
      latestB = bSnap;

      await openGame(session.pageA);
      await downloadSnapshot(session.pageA, bSnap);
      expect(existsSync(localArchivePath(seeded.deviceA.appDataDir, bSnap))).toBe(true);
      await applySnapshot(session.pageA, bSnap);
      expect(await readSave(seeded.deviceA)).toBe(bSave);
    }

    // A device head tracks that device's last published progress; applying the
    // other device's snapshot does not republish the head.
    await expectDeviceHeadEventually(seeded.cloudRoot, DEVICE_A_ID, latestA);
    await expectDeviceHeadEventually(seeded.cloudRoot, DEVICE_B_ID, latestB);
    expect(await readSave(seeded.deviceA)).toBe(`round-${ROUNDS}-from-b\n`);
    expect(await readSave(seeded.deviceB)).toBe(`round-${ROUNDS}-from-b\n`);
    type Origin = { date: string; device_id?: string; created_at?: number };
    const catalogA = (await readJson(join(seeded.deviceA.archiveRoot, STORAGE_KEY, 'Backups.json')))
      .backups as Origin[];
    const catalogB = (await readJson(join(seeded.deviceB.archiveRoot, STORAGE_KEY, 'Backups.json')))
      .backups as Origin[];
    const remote = (await readJson(cloudPaths(seeded.cloudRoot).manifest)).games as Record<
      string,
      { snapshots: Record<string, Origin> }
    >;
    for (const id of published) {
      expect(existsSync(cloudArchivePath(seeded.cloudRoot, id))).toBe(true);
      expect(existsSync(localArchivePath(seeded.deviceA.appDataDir, id))).toBe(true);
      expect(existsSync(localArchivePath(seeded.deviceB.appDataDir, id))).toBe(true);
      const original = catalogA.find((snapshot) => snapshot.date === id)!;
      expect(original.device_id).toBe(creators.get(id));
      for (const copy of [
        catalogB.find((snapshot) => snapshot.date === id)!,
        remote[STORAGE_KEY]!.snapshots[id]!,
      ]) {
        expect(copy.device_id).toBe(original.device_id);
        expect(copy.created_at).toBe(original.created_at);
      }
    }
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
