import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

const imagePattern = /\.(png|jpe?g|gif|webp|svg|avif)$/i;
export function isDocumentationImage(file) {
  return (
    imagePattern.test(file) &&
    (file.startsWith("docs/screenshots/") ||
      file.startsWith("apps/rgsm-docs/static/img/"))
  );
}

export function findUnreferencedImages(images, documents) {
  const referenced = new Set();
  for (const [file, source] of documents) {
    // Examples and comments do not display images in the documentation.
    const text = source
      .replace(/<!--[\s\S]*?-->/g, "")
      .replace(/^\s*(`{3,}|~{3,})[^\n]*\n[\s\S]*?^\s*\1\s*$/gm, "");
    const destinations = [
      ...text.matchAll(/!?\[[^\]\n]*\]\(\s*(<[^>]+>|[^\s)]+)(?:\s+[^)]*)?\)/g),
      ...text.matchAll(/^\s*\[[^\]\n]+\]:\s*(<[^>]+>|\S+)/gm),
      ...text.matchAll(/\bsrc=["']([^"']+)["']/g),
    ];
    for (const match of destinations) {
      const target = decodeURIComponent(
        match[1].replace(/^<|>$/g, "").split(/[?#]/)[0],
      );
      if (/^[a-z]+:/i.test(target)) continue;
      const resolved = target.startsWith("/img/")
        ? `apps/rgsm-docs/static${target}`
        : path.posix.normalize(
            path.posix.join(path.posix.dirname(file), target),
          );
      referenced.add(resolved);
    }
  }
  return images.filter((file) => !referenced.has(file));
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href
) {
  const base = process.argv[2];
  if (!base)
    throw new Error("Usage: node scripts/check-doc-images.mjs <base-ref>");
  const git = (...args) =>
    execFileSync("git", args, { encoding: "utf8" }).split("\0").filter(Boolean);
  const images = git(
    "diff",
    "--name-only",
    "-z",
    "--diff-filter=AM",
    "--no-renames",
    base,
    "--",
  ).filter(isDocumentationImage);
  const documents = git("ls-files", "-z", "--", "*.md", "*.mdx")
    .filter(
      (file) =>
        file.startsWith("docs/") ||
        file.startsWith("apps/rgsm-docs/") ||
        /^README[^/]*\.md$/i.test(file),
    )
    .map((file) => [file, readFileSync(file, "utf8")]);
  const missing = findUnreferencedImages(images, documents);
  if (missing.length) {
    console.error(
      "Documentation images must be referenced by repository documentation (PR descriptions do not count):\n" +
        missing.join("\n"),
    );
    process.exitCode = 1;
  } else {
    console.log(
      `Documentation image references verified (${images.length} added or modified images).`,
    );
  }
}
