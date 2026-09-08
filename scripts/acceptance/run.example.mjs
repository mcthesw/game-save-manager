import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import { withSession } from "__SESSION_MODULE__";
import {
  seedEmptyCloudWithLocalGame,
  deviceLayout,
  writeSave,
} from "__FIXTURE_MODULE__";
import { buildExample, startExampleDevice } from "__EXAMPLE_MODULE__";

await withSession(import.meta.url, async (session) => {
  if (process.platform !== "win32" || session.metadata.target !== "win32") {
    throw new Error(
      "This example is verified on Windows only; it does not provision other platforms",
    );
  }
  const artifacts = await buildExample(session);
  await session.prepare(async ({ dataDir }) => {
    // Freely replace the seed/assets for the change. Do not pre-execute the acceptance actions.
    const { deviceA, deviceB } = await seedEmptyCloudWithLocalGame(dataDir);
    await writeSave(deviceA, "A: starting progress\n");
    await writeSave(deviceB, "B: starting progress\n");
  });
  const entries = [];
  // Delete 'b' for a one-device journey; fixture layout is stable across restarts.
  for (const key of ["a", "b"]) {
    const device = deviceLayout(session.dataDir, key);
    const entry = await startExampleDevice(session, device, artifacts);
    entries.push(entry);
    console.log(
      `${entry.name}\n  GUI: ${entry.gui}\n  Text panel: ${entry.panel}`,
    );
  }
  await writeFile(
    join(session.root, "entrypoints.json"),
    JSON.stringify(entries, null, 2) + "\n",
  );
  console.log(
    "Ready for manual use; no acceptance result has been recorded. Ctrl+C stops this session and preserves data.",
  );
  await session.wait();
});
