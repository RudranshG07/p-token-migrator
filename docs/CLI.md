# CLI binaries

Three Rust binaries cover the platform's command-line surface. All three
share the same workspace; build once with `cargo build --release` (or
`--workspace`) and the binaries land under `target/release/`.

## `sta` — Solana token-program analyzer

Static analysis. Parses Rust source with `syn`, finds SPL Token /
Token-2022 / `token_interface` CPI call sites, emits a manifest (and
optionally SARIF for GitHub code-scanning).

```bash
# Scan a project, print summary to stderr, write manifest + SARIF.
sta scan samples/anchor-token-vault \
    --out data/manifest.json \
    --sarif-out data/p-token.sarif \
    --summary

# Use a measured CU profile produced by cu-bench.
sta scan path/to/project \
    --profile-path profiles/measured-spl-token.json

# CI-friendly — exit 2 if any finding is flagged for review.
sta scan path/to/project --fail-on-review

# Server-side: read source files from stdin as JSON.
echo '{"protocol":"foo","files":[{"relative":"src/lib.rs","content":"..."}]}' \
    | sta scan-sources
```

Manifest output is camelCase JSON compatible with the dashboard server's
expected shape. See `crates/solana-token-analyzer/src/manifest.rs` for
the exact field types.

## `cu-bench` — compute-unit measurement

Spins up LiteSVM, runs each canonical token-program instruction, captures
the exact compute units consumed, and emits a `BenchmarkProfile` JSON that
`sta scan` consumes via `--profile-path`.

```bash
# Measure SPL Token and write the profile.
cu-bench --out profiles/measured-spl-token.json

# Also measure Token-2022 (output is informational; profile shape still
# carries legacy + p-token fields only).
cu-bench --out profiles/measured-spl-token.json --include-token-2022
```

p-token CU values in the output are currently a 96% reduction placeholder.
Once the canonical p-token program is available, extend `measure_p_token()`
in `crates/cu-bench/src/main.rs` to record measured values.

## `replay` — forked-mainnet replay + two-build diff

Fetches real mainnet transactions, snapshots every referenced account into
LiteSVM, deploys user programs from the BPF Loader Upgradeable format,
replays, and reports diffed outcomes.

```bash
# Single-build replay: replay recent SPL Token txs and diff against mainnet.
replay \
    --rpc https://your.rpc.url/ \
    --program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA \
    --limit 25 \
    --out data/replay.json

# Two-build diff: replay each tx against your legacy and new ELF; diff outcomes.
replay \
    --rpc https://your.rpc.url/ \
    --program YourProgram111111111111111111111111111111111 \
    --legacy-so target/legacy.so \
    --new-so target/new.so \
    --limit 25 \
    --out data/diff.json
```

The public mainnet RPC rate-limits aggressively; use a paid provider
(Helius / Triton / QuickNode) for any non-trivial run.

The diff report's `legacyVsNew` array is the field a protocol team scans
first when deciding whether a migration is safe to push.

## Honest limitations

- **Replay synthetic signer.** Non-payer signers can't be impersonated, so
  programs gating on a non-payer signer will fail in replay. The failure is
  surfaced as a divergence — it is signal, not a bug.
- **Historical state.** RPC returns finalized account state, not the exact
  pre-tx slot's state. State drift is surfaced as a divergence.
- **Address Lookup Tables.** Resolved automatically. Pass `--skip-alt` is
  not currently exposed in the CLI; set it via the library API if needed.

## CI integration

See [docs/GITHUB_ACTION.md](GITHUB_ACTION.md) for copy-pasteable workflow
recipes covering SARIF upload, PR comments, fail-on-review gating, and
two-build replay diff.
