import { test } from "node:test";
import assert from "node:assert/strict";
import {
  findUnreferencedImages,
  isDocumentationImage,
} from "./check-doc-images.mjs";

const image = "docs/screenshots/example.png";
test("requires actual references, not filenames in prose, comments, or code", () => {
  const source =
    "example.png\n<!-- ![example](screenshots/example.png) -->\n```md\n![example](screenshots/example.png)\n```";
  assert.deepEqual(
    findUnreferencedImages([image], [["docs/guide.md", source]]),
    [image],
  );
});
test("resolves relative references from translated guides", () => {
  assert.deepEqual(
    findUnreferencedImages(
      [image],
      [
        [
          "apps/rgsm-docs/i18n/en/docusaurus-plugin-content-docs/current/games/paths.md",
          "![example](../../../../../../../docs/screenshots/example.png)",
        ],
      ],
    ),
    [],
  );
});
test("supports static site images and reference-style links", () => {
  const staticImage = "apps/rgsm-docs/static/img/guide/example.png";
  assert.deepEqual(
    findUnreferencedImages(
      [image, staticImage],
      [
        [
          "docs/guide.md",
          '[example]: screenshots/example.png\n![example][]\n<img src="/img/guide/example.png" />',
        ],
      ],
    ),
    [],
  );
});
test("remote PR image URLs do not count as local documentation references", () => {
  assert.deepEqual(
    findUnreferencedImages(
      [image],
      [
        [
          "docs/guide.md",
          "![example](https://raw.githubusercontent.com/owner/repo/old/docs/screenshots/example.png)",
        ],
      ],
    ),
    [image],
  );
});
test("checks documentation assets without including application assets", () => {
  assert.equal(isDocumentationImage(image), true);
  assert.equal(
    isDocumentationImage("apps/rgsm-docs/static/img/guide/example.webp"),
    true,
  );
  assert.equal(
    isDocumentationImage("apps/rgsm-gui/src/assets/icon.png"),
    false,
  );
});
