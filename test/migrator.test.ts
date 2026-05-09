import test from "node:test";
import assert from "node:assert/strict";
import { getSampleProjectPath, scanProject } from "../src/migrator.ts";

test("sample project produces a migration manifest", async () => {
  const manifest = await scanProject(getSampleProjectPath(), { protocol: "Test Vault" });

  assert.equal(manifest.protocol, "Test Vault");
  assert.ok(manifest.totals.filesScanned >= 2);
  assert.ok(manifest.totals.callSites >= 3);
  assert.ok(manifest.totals.savedCu > 0);
  assert.ok(manifest.findings.some((finding) => finding.operation === "transfer"));
  assert.ok(["passed", "review_required"].includes(manifest.simulation.status));
});
