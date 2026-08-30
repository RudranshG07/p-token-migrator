import test from "node:test";
import assert from "node:assert/strict";
import { buildMigrationBundle } from "../src/codegen.ts";
import { buildReportSummary, DEFAULT_BENCHMARK_PROFILE, getSampleProjectPath, scanProject, scanSourceFiles } from "../src/migrator.ts";

test("sample project produces a migration manifest", async () => {
  const manifest = await scanProject(getSampleProjectPath(), { protocol: "Test Vault" });

  assert.equal(manifest.protocol, "Test Vault");
  assert.ok(manifest.totals.filesScanned >= 2);
  assert.ok(manifest.totals.callSites >= 3);
  assert.ok(manifest.totals.savedCu > 0);
  assert.ok(manifest.findings.some((finding) => finding.operation === "transfer"));
  assert.ok(["passed", "review_required"].includes(manifest.simulation.status));
  assert.equal(manifest.simulation.mode, "deterministic");
  assert.ok(manifest.milestones.some((milestone) => milestone.name === "AST Scanner"));
  assert.ok(manifest.findings.every((finding) => finding.tokenProgram === "spl_token"));
});

test("report summaries do not include source snippets", async () => {
  const manifest = await scanSourceFiles([
    {
      relative: "programs/vault/src/lib.rs",
      content: "use anchor_spl::token;\npub fn deposit() { token::transfer(cpi_ctx, amount).unwrap(); }"
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
      content: "use anchor_spl::token;\npub fn deposit() { token::transfer(cpi_ctx, amount).unwrap(); }"
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

test("custom benchmark profiles control compute estimates", async () => {
  const profile = structuredClone(DEFAULT_BENCHMARK_PROFILE);
  profile.name = "test-profile";
  profile.operations.transfer = { legacyCu: 1000, pTokenCu: 100 };

  const manifest = await scanSourceFiles([
    {
      relative: "programs/vault/src/lib.rs",
      content: "use anchor_spl::token;\npub fn deposit() { token::transfer(cpi_ctx, amount).unwrap(); }"
    }
  ], { protocol: "Profile Vault", benchmarkProfile: profile });

  assert.equal(manifest.pTokenProfile.name, "test-profile");
  assert.equal(manifest.totals.legacyCu, 1000);
  assert.equal(manifest.totals.pTokenCu, 100);
  assert.equal(manifest.totals.savedCu, 900);
});

test("scanner ignores comments and strings", async () => {
  const manifest = await scanSourceFiles([
    {
      relative: "programs/vault/src/lib.rs",
      content: [
        "pub fn ignored() {",
        "  // token::transfer(cpi_ctx, amount)?;",
        "  let text = \"token::burn(cpi_ctx, amount)?\";",
        "  /* token::mint_to(cpi_ctx, amount)?; */",
        "}"
      ].join("\n")
    }
  ], { protocol: "Comment Vault" });

  assert.equal(manifest.totals.callSites, 0);
});

test("scanner detects aliased token modules", async () => {
  const manifest = await scanSourceFiles([
    {
      relative: "programs/vault/src/lib.rs",
      content: [
        "use anchor_spl::token as spl_token_cpi;",
        "use spl_token::instruction as token_ix;",
        "pub fn run() {",
        "  spl_token_cpi::burn(cpi_ctx, amount)?;",
        "  token_ix::approve(program_id, source, delegate, owner, &[], amount)?;",
        "}"
      ].join("\n")
    }
  ], { protocol: "Alias Vault" });

  assert.equal(manifest.totals.callSites, 2);
  assert.ok(manifest.findings.some((finding) => finding.operation === "burn"));
  assert.ok(manifest.findings.some((finding) => finding.operation === "approve"));
});

test("migration bundle contains codegen and shim artifacts", async () => {
  const manifest = await scanSourceFiles([
    {
      relative: "programs/vault/src/lib.rs",
      content: "use anchor_spl::token;\npub fn deposit() { token::transfer(cpi_ctx, amount).unwrap(); }"
    }
  ], { protocol: "Bundle Vault" });

  const bundle = buildMigrationBundle(manifest);

  assert.equal(bundle.protocol, "Bundle Vault");
  assert.equal(bundle.summary.callSites, 1);
  assert.ok(bundle.files.some((file) => file.path === "migration-plan.json"));
  assert.ok(bundle.files.some((file) => file.path === "patches/p-token-replacements.rs" && file.content.includes("p_token_shim::transfer")));
  assert.ok(bundle.files.some((file) => file.path.includes("p-token-shim-anchor")));
});
