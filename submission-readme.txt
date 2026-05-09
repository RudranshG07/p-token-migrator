
About the Project
-----------------
The p-token Migration Toolkit is a Rust CLI that helps Solana protocols
migrate from the legacy SPL Token program to p-token (SIMD-0266) — the
new token program that delivers a 95–98% reduction in token-transfer
compute units.

When p-token hits mainnet later in 2026, every major protocol on Solana
(Jupiter, Kamino, Marginfi, Drift, and the long tail) will need to
migrate. There is currently no tooling to make that migration safe,
measurable, or reversible. This toolkit fills that gap.

Components:
  1. IDL Scanner — parses an Anchor program's IDL and source, identifies
     every SPL Token CPI call site, emits a migration manifest.
  2. p-token Codegen + CU Diff — auto-generates the p-token equivalent
     for each call site with side-by-side compute-unit comparisons.
  3. Forked-Mainnet Dry-Run Simulator — replays both versions against
     real mainnet state and flags any behavioral divergence.
  4. Compatibility Shim Anchor Crate — lets programs support both token
     programs simultaneously during the transition window.
  5. Migration Dashboard — public CU savings tracker per protocol.






Milestones
----------
   IDL Scanner
   p-token Codegen + CU Diff
  Forked-Mainnet Dry-Run Simulator
   Compatibility Shim Anchor Crate (crates.io)
 Migration Dashboard + Public Launch


Primary KPI
-----------
Number of Solana programs that complete an end-to-end p-token migration
using the toolkit. Target: 5+ protocols within 60 days of p-token
mainnet activation. Secondary: aggregate compute units saved across
migrated programs, measured via on-chain telemetry.



