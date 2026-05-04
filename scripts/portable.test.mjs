import assert from "node:assert/strict";
import test from "node:test";

import {
  isReleaseAssetNameConflict,
  uploadReleaseAssetWithClobber,
} from "./portable.mjs";

function releaseAssetConflictError() {
  return {
    status: 422,
    response: {
      data: {
        errors: [
          {
            resource: "ReleaseAsset",
            code: "already_exists",
            field: "name",
          },
        ],
      },
    },
  };
}

test("detects GitHub release asset name conflicts", () => {
  assert.equal(isReleaseAssetNameConflict(releaseAssetConflictError()), true);
  assert.equal(
    isReleaseAssetNameConflict({
      status: 422,
      response: {
        data: {
          errors: [{ resource: "ReleaseAsset", code: "missing_field" }],
        },
      },
    }),
    false,
  );
  assert.equal(isReleaseAssetNameConflict(new Error("network failed")), false);
});

test("uploads a release asset once when there is no conflict", async () => {
  const calls = [];
  const github = {
    rest: {
      repos: {
        uploadReleaseAsset: async (payload) => {
          calls.push(["upload", payload.name]);
          return { ok: true };
        },
        listReleaseAssets: async () => {
          throw new Error("should not list release assets");
        },
        deleteReleaseAsset: async () => {
          throw new Error("should not delete release assets");
        },
      },
    },
  };

  const result = await uploadReleaseAssetWithClobber({
    github,
    options: { owner: "mcthesw", repo: "game-save-manager" },
    releaseId: "123",
    name: "RGSM_1.9.0_x64-portable-slim.zip",
    data: Buffer.from("zip"),
    log: () => {},
  });

  assert.deepEqual(result, { ok: true });
  assert.deepEqual(calls, [["upload", "RGSM_1.9.0_x64-portable-slim.zip"]]);
});

test("deletes the existing release asset and retries after a name conflict", async () => {
  const calls = [];
  let uploadCount = 0;
  const github = {
    rest: {
      repos: {
        uploadReleaseAsset: async (payload) => {
          uploadCount += 1;
          calls.push(["upload", payload.name]);
          if (uploadCount === 1) {
            throw releaseAssetConflictError();
          }
          return { ok: true };
        },
        listReleaseAssets: async (payload) => {
          calls.push(["list", payload.release_id]);
          return {
            data: [
              { id: 7, name: "LICENSE" },
              { id: 42, name: "RGSM_1.9.0_x64-portable-slim.zip" },
            ],
          };
        },
        deleteReleaseAsset: async (payload) => {
          calls.push(["delete", payload.asset_id]);
        },
      },
    },
  };

  const result = await uploadReleaseAssetWithClobber({
    github,
    options: { owner: "mcthesw", repo: "game-save-manager" },
    releaseId: "123",
    name: "RGSM_1.9.0_x64-portable-slim.zip",
    data: Buffer.from("zip"),
    log: () => {},
  });

  assert.deepEqual(result, { ok: true });
  assert.deepEqual(calls, [
    ["upload", "RGSM_1.9.0_x64-portable-slim.zip"],
    ["list", "123"],
    ["delete", 42],
    ["upload", "RGSM_1.9.0_x64-portable-slim.zip"],
  ]);
});
