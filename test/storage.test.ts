import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { scanSourceFiles } from "../src/migrator.ts";
import { createFileJobStore } from "../src/storage.ts";

test("file job store saves, lists, and finds jobs", async () => {
  const dir = await mkdtemp(path.join(os.tmpdir(), "p-token-store-"));
  try {
    const store = createFileJobStore(path.join(dir, "jobs.json"));
    const manifest = await scanSourceFiles([
      {
        relative: "programs/vault/src/lib.rs",
        content: "use anchor_spl::token;\npub fn deposit() { token::transfer(cpi_ctx, amount).unwrap(); }"
      }
    ], { protocol: "Stored Vault" });

    const saved = await store.save(manifest, { storeManifest: false, retentionLimit: 10 });
    const jobs = await store.list();
    const found = await store.find(saved.id);

    assert.equal(jobs.length, 1);
    assert.equal(found?.id, saved.id);
    assert.equal(found?.manifest, undefined);
    assert.equal(found?.report.operations.transfer, 1);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});
