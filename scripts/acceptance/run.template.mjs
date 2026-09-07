import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import { withSession } from "__SESSION_MODULE__";

// Replace this seed with the artificial files/configuration needed for this change.
// Keep setup separate from the actions the reviewer is meant to perform in RGSM.
await withSession(import.meta.url, async (session) => {
  if (process.platform !== session.metadata.target) {
    throw new Error(
      `Run this packet on ${session.metadata.target}; current platform is ${process.platform}`,
    );
  }
  await session.prepare(async ({ dataDir }) => {
    await writeFile(
      join(dataDir, "progress.txt"),
      "artificial starting progress\n",
      { flag: "wx" },
    );
  });

  // Add the task-specific launcher here. For long-running processes:
  // session.start(command, args, { cwd, env });
  // await an actual readiness check, then print the reviewer entry point
  // await session.wait(); // Ctrl+C stops owned processes and retains data
  console.log(`Fixture directory: ${session.dataDir}`);
  console.log(
    "Scaffold only: no application was started and no acceptance has been performed",
  );
});
