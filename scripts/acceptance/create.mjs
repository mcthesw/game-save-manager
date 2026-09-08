import { mkdir, readFile, writeFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repository = resolve(scriptDir, "../..");

export async function createPacket(
  name,
  { repoRoot = repository, target = process.platform, example = false } = {},
) {
  if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(name ?? "")) {
    throw new Error("Use a lowercase packet name, for example save-restore");
  }
  if (!["linux", "win32", "darwin"].includes(target))
    throw new Error("Target must be linux, win32 or darwin");
  if (example && target !== "win32")
    throw new Error("The runnable example currently targets win32");
  const root = join(repoRoot, ".rgsm-dev", "acceptance", name);
  await mkdir(dirname(root), { recursive: true });
  // Never overwrite an earlier acceptance session, including reviewer edits.
  await mkdir(root);
  const git = (...args) =>
    execFileSync("git", args, {
      cwd: repoRoot,
      encoding: "utf8",
      windowsHide: true,
    }).trim();
  const metadata = {
    kind: "rgsm-acceptance",
    name,
    revision: git("rev-parse", "HEAD"),
    dirty: Boolean(git("status", "--porcelain", "--untracked-files=normal")),
    preparedOn: process.platform,
    target,
    createdAt: new Date().toISOString(),
  };
  await writeFile(
    join(root, "session.json"),
    JSON.stringify(metadata, null, 2) + "\n",
  );
  const modulePath = (path) =>
    relative(root, join(repoRoot, path)).replaceAll("\\", "/");
  const starter = await readFile(
    join(scriptDir, example ? "run.example.mjs" : "run.template.mjs"),
    "utf8",
  );
  await writeFile(
    join(root, "run.mjs"),
    starter
      .replace(
        "__SESSION_MODULE__",
        modulePath("scripts/acceptance/session.mjs"),
      )
      .replace(
        "__FIXTURE_MODULE__",
        modulePath("apps/rgsm-gui/e2e/support/cloud-fixture.ts"),
      )
      .replace(
        "__EXAMPLE_MODULE__",
        modulePath("apps/rgsm-gui/scripts/acceptance/example.mjs"),
      ),
  );
  await writeFile(
    join(root, "ACCEPTANCE.md"),
    await readFile(
      join(scriptDir, example ? "example.md" : "packet.template.md"),
    ),
  );
  await mkdir(join(root, "evidence"));
  return root;
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  const args = process.argv.slice(2);
  const example = args.includes("--example");
  const [name, target, ...extra] = args.filter((arg) => arg !== "--example");
  try {
    if (extra.length || !name)
      throw new Error(
        "Usage: pnpm acceptance:new <name> [linux|win32|darwin] [--example]",
      );
    console.log(await createPacket(name, { target, example }));
    console.log(
      "Scaffold only: adapt ACCEPTANCE.md and run.mjs before offering this packet for review",
    );
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
