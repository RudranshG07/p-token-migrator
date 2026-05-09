# p-token Migration Toolkit

A fullstack toolkit for planning Solana Anchor migrations from legacy SPL Token CPIs to p-token-compatible call sites.

The app includes:

- A browser dashboard for uploading an Anchor project folder, scanning the included sample, or scanning a trusted server path in local mode.
- A TypeScript Node backend API that scans Rust and IDL files, emits a migration manifest, estimates compute-unit savings, and records protocol runs.
- A Rust CLI with the same core workflow for terminal usage.
- A TypeScript browser dashboard and an Anchor-style program with SPL Token CPI call sites.

## Run the Fullstack App

```bash
npm run dev
```

Open `http://localhost:4173`.

## Deploy

```bash
docker compose up --build
```

Production deployments should keep `ALLOW_SERVER_PATH_SCAN=0` and use browser uploads through `/api/scan-sources`.
If `API_KEY` is configured, users can enter it in the dashboard before starting a scan.

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
5. Persist scan jobs under the configured `JOB_STORE_PATH`.
6. Render a dashboard with aggregate CU savings, protocol runs, findings, replacement guidance, downloadable manifests, and shareable report summaries.

For deployment and API details, see:

- [Deployment](docs/DEPLOYMENT.md)
- [API](docs/API.md)
- [Security](docs/SECURITY.md)
- [Storage](docs/STORAGE.md)
- [Scanner](docs/SCANNER.md)

p-token/SIMD-0266 mainnet behavior is represented by an explicit estimator profile until canonical on-chain program IDs and final interfaces are available.
