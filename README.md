# p-token Migration Toolkit

A fullstack toolkit for planning Solana Anchor migrations from legacy SPL Token CPIs to p-token-compatible call sites.

The app includes:

- A React TSX browser dashboard for uploading an Anchor project folder, scanning the included sample, or scanning a trusted server path in local mode.
- A TypeScript Node backend API that scans Rust and IDL files, emits a migration manifest, estimates compute-unit savings, and records protocol runs.
- A Rust CLI with the same core workflow for terminal usage.
- A neo-brutalist frontend inspired by `marieooq/neo-brutalism-ui-library`, with high-contrast panels, hard shadows, keyboard-visible controls, and responsive report pages.

## Run the Fullstack App

```bash
npm install
npm run dev
```

Open `http://localhost:4173`.

`npm run dev` builds the TSX frontend into `web-dist/` and starts the TypeScript Node API.

Routes:

- `/` landing page
- `/app` scanner dashboard
- `/docs` developer docs
- `/reports/:id` public report

## Build

```bash
npm run typecheck
npm run build
```

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

Or use the persistent interactive CLI:

```bash
npm run cli -- interactive
```

For a guided prompt flow:

```bash
npm run cli -- wizard
```

Or scan another local project:

```bash
npm run cli -- scan /path/to/anchor/project --out data/manifest.json
```

Generate a full migration bundle:

```bash
npm run cli -- scan /path/to/anchor/project --bundle-out data/migration-bundle --summary
```

Generate SARIF for code scanning:

```bash
npm run cli -- scan /path/to/anchor/project --sarif-out data/p-token.sarif --summary
```

## Test

```bash
npm run typecheck
npm run build
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
- [CLI](docs/CLI.md)
- [Security](docs/SECURITY.md)
- [Storage](docs/STORAGE.md)
- [Scanner](docs/SCANNER.md)
- [Milestones](docs/MILESTONES.md)

p-token/SIMD-0266 mainnet behavior is represented by an explicit estimator profile until canonical on-chain program IDs and final interfaces are available.
