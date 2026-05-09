# p-token Migration Toolkit

A local fullstack toolkit for planning Solana Anchor migrations from legacy SPL Token CPIs to p-token-compatible call sites.

The app includes:

- A browser dashboard for scanning the included Anchor program or a local project path.
- A TypeScript Node backend API that scans Rust and IDL files, emits a migration manifest, estimates compute-unit savings, and records protocol runs.
- A Rust CLI with the same core workflow for terminal usage.
- A TypeScript browser dashboard and an Anchor-style program with SPL Token CPI call sites.

## Run the Fullstack App

```bash
npm run dev
```

Open `http://localhost:4173`.

## Run the Rust CLI

```bash
npm run cli:sample
```

Or scan another local project:

```bash
npm run cli -- scan /path/to/anchor/project --out data/manifest.json
```

## Test

```bash
npm test
cargo test --manifest-path cli/Cargo.toml
```

## Current Flow

1. Scan Rust source and Anchor IDL files for SPL Token CPI usage.
2. Classify call sites such as transfer, mint, burn, approve, close account, and initialize account.
3. Produce a migration manifest with file, line, operation, confidence, legacy CU estimate, p-token CU estimate, savings, risk level, and replacement guidance.
4. Run a deterministic dry-run simulator that flags behavioral divergence risks.
5. Persist scan jobs under `data/jobs.json`.
6. Render a dashboard with aggregate CU savings, protocol runs, findings, replacement guidance, and downloadable manifests.

This is a local development prototype. p-token/SIMD-0266 mainnet behavior is represented by an explicit estimator profile until canonical on-chain program IDs and final interfaces are available.
