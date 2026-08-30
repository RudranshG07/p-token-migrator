# solana-token-analyzer

A toolchain for Solana protocols evaluating SPL Token / Token-2022 / p-token
migrations. The product is the **engine + replay**, not the dashboard.

> **Note:** p-token / SIMD-0266 mainnet behavior is represented by an explicit
> estimator profile until canonical on-chain program IDs and final interfaces
> are available.

```
crates/solana-token-analyzer   AST scanner (syn) + SARIF + migration manifest
crates/cu-bench                LiteSVM compute-unit measurement harness
crates/replay                  Forked-mainnet replay + two-build diff
crates/p-token-shim-anchor     Anchor crate for transition routing
src/, web/                     TS server + React dashboard (one surface, not the product)
```

## What it does

1. **Scan** an Anchor / native Solana program. Parse every Rust file with
   `syn`, resolve `use` aliases, find every SPL Token / Token-2022 /
   `token_interface` CPI call site, tag with risk level. Emit a manifest
   and (optionally) SARIF for GitHub code scanning.
2. **Measure** compute units. The `cu-bench` binary spins up LiteSVM, runs
   each canonical token operation, and records `compute_units_consumed`
   exactly. Numbers shown by the dashboard are measured, not estimated.
3. **Replay** real mainnet transactions against forked state in LiteSVM.
   Resolves Address Lookup Tables, deploys user programs through the
   BPF Loader Upgradeable format, and reports outcomes diffed against
   mainnet's recorded result.
4. **Diff two builds.** Replay each mainnet tx against your *legacy* and
   *new* program ELF and report side-by-side outcomes. This is the
   "is my migration safe to push" primitive.

## Quick start

```bash
# Scan a project
cargo run -p solana-token-analyzer --bin sta -- \
    scan samples/anchor-token-vault --summary

# Measure CU costs of real SPL Token instructions
cargo run -p cu-bench --release -- --out profiles/measured-spl-token.json

# Replay recent mainnet txs of SPL Token (use a paid RPC for serious work)
cargo run -p replay --release -- \
    --rpc https://api.mainnet-beta.solana.com \
    --program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA \
    --limit 5

# Two-build diff (when you have both ELFs)
cargo run -p replay --release -- \
    --rpc $RPC --program YourProgram... \
    --legacy-so /path/to/legacy.so --new-so /path/to/new.so \
    --limit 25 --out diff.json
```

## Dashboard

The TypeScript Node server is a thin surface on top of the Rust engine — useful
for product teams that want to upload source through a UI. The engine binary
is the source of truth for every number.

```bash
npm install
npm run dev
```

Open `http://localhost:4173`. Routes: `/`, `/app`, `/docs`, `/reports/:id`.

Production:
```bash
docker compose up --build
```

## GitHub Action

The most common deployment surface — drop a workflow into your protocol repo
that runs the analyzer on every PR, uploads SARIF for code-scanning
annotations, and (optionally) runs two-build replay diff on the proposed
upgrade.

See [docs/GITHUB_ACTION.md](docs/GITHUB_ACTION.md) for copy-pasteable recipes.

## Test

```bash
cargo test --workspace
npm run typecheck && npm test
```

## Status of the moving parts

| Component | State |
|---|---|
| AST scanner (Rust) | Working — Anchor + native + `token_interface` + Token-2022 |
| SARIF output | Working — GitHub code-scanning compatible |
| CU measurement harness | Working — SPL Token measured, Token-2022 optional |
| Replay infrastructure | Working — ALT resolution + BPF Loader Upgradeable program install |
| Two-build diff | Working — `replay --legacy-so ... --new-so ...` |
| p-token replacement numbers | Placeholder 96% reduction — replaced with measured values once a canonical p-token build is available |
| Postgres job store | Not yet — current store is a JSON file |
| Marketplace GitHub Action | Not yet — recipes build from source via `cargo install` |

## Real-world scans

The analyzer has been validated against three production Solana protocols:

| Protocol | Style | Findings | Saved CU |
|---|---|---|---|
| marginfi-v2 | Anchor + token_interface + token_2022 | 74 | 368,520 |
| drift-v2 | Anchor + token_interface (transfer hooks) | 74 | 376,700 |
| phoenix-v1 | Native + spl_token::instruction | 88 | 476,760 |

Full breakdowns plus wrapper-detection examples: [docs/CASE_STUDIES.md](docs/CASE_STUDIES.md).

## Docs

- [Case studies](docs/CASE_STUDIES.md) — marginfi, drift, phoenix scans
- [GitHub Action](docs/GITHUB_ACTION.md) — CI integration recipes
- [Deployment](docs/DEPLOYMENT.md) — server / Docker
- [API](docs/API.md) — dashboard server endpoints
- [CLI](docs/CLI.md) — `sta` binary
- [Security](docs/SECURITY.md)
- [Storage](docs/STORAGE.md)
- [Scanner](docs/SCANNER.md)

## License

MIT.
