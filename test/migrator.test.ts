import test from "node:test";
import assert from "node:assert/strict";
import { buildReportSummary, getSampleProjectPath, scanProject, scanSourceFiles } from "../src/migrator.ts";

test("sample project produces a migration manifest", async () => {
  const manifest = await scanProject(getSampleProjectPath(), { protocol: "Test Vault" });

  assert.equal(manifest.protocol, "Test Vault");
  assert.ok(manifest.totals.filesScanned >= 2);
  assert.ok(manifest.totals.callSites >= 3);
  assert.ok(manifest.totals.savedCu > 0);
  assert.ok(manifest.findings.some((finding) => finding.operation === "transfer"));
  assert.ok(["passed", "review_required"].includes(manifest.simulation.status));
});

test("report summaries do not include source snippets", async () => {
  const manifest = await scanSourceFiles([
    {
      relative: "programs/vault/src/lib.rs",
      content: "pub fn deposit() { token::transfer(cpi_ctx, amount).unwrap(); }"
    }
  ], { protocol: "Report Vault" });

  const report = buildReportSummary(manifest, "job_test");
  const serialized = JSON.stringify(report);

  assert.equal(report.id, "job_test");
  assert.equal(report.operations.transfer, 1);
  assert.equal(report.files[0]?.file, "programs/vault/src/lib.rs");
  assert.equal(serialized.includes("token::transfer"), false);
  assert.equal(serialized.includes("replacementPatch"), false);
});

test("uploaded sources can be scanned without server filesystem access", async () => {
  const manifest = await scanSourceFiles([
    {
      relative: "programs/vault/src/lib.rs",
      content: "pub fn deposit() { token::transfer(cpi_ctx, amount).unwrap(); }"
    },
    {
      relative: "idls/vault.json",
      content: "{\"name\":\"tokenProgram\"}"
    }
  ], { protocol: "Uploaded Vault" });

  assert.equal(manifest.protocol, "Uploaded Vault");
  assert.equal(manifest.root, "uploaded-sources");
  assert.equal(manifest.totals.callSites, 1);
  assert.equal(manifest.idlHints.length, 1);
});
