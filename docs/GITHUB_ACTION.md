# GitHub Action — continuous SPL Token / Token-2022 / p-token analysis

This guide shows how a Solana protocol team wires `solana-token-analyzer`
into their GitHub repository so every pull request gets:

1. Inline code-scanning annotations for SPL Token / Token-2022 CPI call sites
   (powered by SARIF).
2. A markdown summary comment on the PR with total CU savings + risk
   breakdown.
3. An optional CI gate that fails the build when any finding requires manual
   review.

Pick the recipe that matches how much wiring you want.

---

## Recipe 0 — drop-in (composite action, one step)

The fastest path. Pin to a tagged release and let the action download a
pre-built `sta` binary for the runner. No Rust toolchain install, no
`cargo install`, no build time.

```yaml
name: p-token analysis

on:
  pull_request:
  push:
    branches: [main]

permissions:
  contents: read
  security-events: write
  pull-requests: write

jobs:
  scan:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - id: analyze
        uses: RudyG07/p-token-migrator@v0
        with:
          project-path: .
          sarif-out: p-token.sarif
          fail-on-review: "false"

      - uses: github/codeql-action/upload-sarif@v3
        with:
          sarif_file: p-token.sarif
          category: solana-token-analyzer

      - name: Job summary
        run: |
          {
            echo "## Solana token-program scan"
            echo "- Call sites: ${{ steps.analyze.outputs.call-sites }}"
            echo "- Saved CU: ${{ steps.analyze.outputs.saved-cu }} (${{ steps.analyze.outputs.savings-percent }}%)"
            echo "- High-risk findings: ${{ steps.analyze.outputs.high-risk-count }}"
          } >> "$GITHUB_STEP_SUMMARY"
```

Inputs (all optional):

| Input | Default | Meaning |
|---|---|---|
| `project-path` | `.` | Directory to scan |
| `manifest-out` | `p-token-manifest.json` | Where to write the JSON manifest |
| `sarif-out` | `` | Write SARIF here (omit to skip) |
| `profile-path` | `` | Use a measured profile (cu-bench output) instead of the bundled estimator |
| `protocol` | (dir name) | Override the protocol name in the manifest |
| `fail-on-review` | `false` | Exit non-zero if any finding needs review |
| `version` | `latest` | Release tag to install |

Outputs:

| Output | Meaning |
|---|---|
| `manifest-path` | Path to the written manifest |
| `call-sites` | Total CPI call sites found |
| `saved-cu` | Total CU saved (legacy − target) |
| `savings-percent` | Aggregate savings percentage |
| `high-risk-count` | Number of high-risk findings |

Runners supported: `ubuntu-latest` (x86_64 Linux), `macos-latest` (arm64
Apple Silicon). Other architectures: use Recipe 1 below to install from
source.

---

## Recipe 1 — install from source (no published release pinning)

Drop this at `.github/workflows/p-token-check.yml` in your protocol repo.

```yaml
name: p-token analysis

on:
  pull_request:
  push:
    branches: [main]

# Required so the workflow can upload SARIF to GitHub code scanning.
permissions:
  contents: read
  security-events: write
  pull-requests: write

jobs:
  scan:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Cache Cargo registry
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Install solana-token-analyzer
        run: |
          cargo install \
            --git https://github.com/RudyG07/p-token-migrator \
            --bin sta \
            --locked

      - name: Run analyzer
        run: |
          sta scan . \
            --out p-token-manifest.json \
            --sarif-out p-token.sarif \
            --summary

      - name: Upload SARIF to code scanning
        uses: github/codeql-action/upload-sarif@v3
        with:
          sarif_file: p-token.sarif
          category: solana-token-analyzer

      - name: Upload manifest as artifact
        uses: actions/upload-artifact@v4
        with:
          name: p-token-manifest
          path: p-token-manifest.json
```

After the first run, findings show up under **Security → Code scanning** in
the repository, and inline on the PR diff as red/yellow annotations.

---

## Recipe 2 — add a PR summary comment

Replace the steps after `Run analyzer` with this to also leave a comment on
the PR. The job summary uses `jq` to format the manifest totals.

```yaml
      - name: Build PR comment body
        id: comment
        run: |
          body=$(jq -r '
            "**Solana token-program analysis**\n\n" +
            "- Protocol: `\(.protocol)`\n" +
            "- Call sites: \(.totals.callSites) across \(.totals.filesScanned) files\n" +
            "- Legacy CU: \(.totals.legacyCu)  →  target: \(.totals.pTokenCu)  " +
              "(saved \(.totals.savedCu), \(.totals.savingsPercent)%)\n" +
            "- High-risk findings: \([.findings[] | select(.risk.level == "high")] | length)\n" +
            "- IDL hints: \(.idlHints | length)\n" +
            "- Profile: `\(.pTokenProfile.name)` (\(.pTokenProfile.status))"
          ' p-token-manifest.json)
          {
            echo "body<<EOF"
            echo "$body"
            echo "EOF"
          } >> "$GITHUB_OUTPUT"

      - name: Comment on PR
        if: github.event_name == 'pull_request'
        uses: peter-evans/create-or-update-comment@v4
        with:
          issue-number: ${{ github.event.pull_request.number }}
          body: ${{ steps.comment.outputs.body }}
```

---

## Recipe 3 — fail the build when findings need review

`sta scan --fail-on-review` exits with status 2 if any finding's risk level
forces a manual review. Drop this into the `Run analyzer` step:

```yaml
      - name: Run analyzer (strict)
        run: |
          sta scan . \
            --out p-token-manifest.json \
            --sarif-out p-token.sarif \
            --fail-on-review \
            --summary
```

Combine with branch protection rules to require a clean scan before merging.

---

## Recipe 4 — forked-mainnet replay diff on PRs

When you have two protocol builds (legacy and proposed), the `replay`
binary can replay recent mainnet txs of the program against both builds and
diff the outcomes. This is the safe-rollout check.

```yaml
  replay:
    runs-on: ubuntu-latest
    needs: scan
    if: github.event_name == 'pull_request'
    steps:
      - uses: actions/checkout@v4

      - name: Build legacy and new protocol binaries
        run: |
          # Adapt to your build system; example shown for cargo build-sbf.
          git checkout main -- programs/
          cargo build-sbf --manifest-path programs/Cargo.toml
          cp target/deploy/your_program.so /tmp/legacy.so

          git checkout ${{ github.event.pull_request.head.sha }} -- programs/
          cargo build-sbf --manifest-path programs/Cargo.toml
          cp target/deploy/your_program.so /tmp/new.so

      - name: Install replay
        run: |
          cargo install \
            --git https://github.com/RudyG07/p-token-migrator \
            --bin replay \
            --locked

      - name: Run two-build diff
        env:
          MAINNET_RPC: ${{ secrets.MAINNET_RPC_URL }}
        run: |
          replay \
            --rpc "$MAINNET_RPC" \
            --program YourProgr1111111111111111111111111111111111 \
            --legacy-so /tmp/legacy.so \
            --new-so /tmp/new.so \
            --limit 25 \
            --out replay-diff.json

      - name: Surface diff in workflow summary
        run: |
          {
            echo "## Two-build replay diff"
            jq -r '"\(.total) txs replayed: \(.matched) matched, \(.diverged) diverged, \(.skipped) skipped"' replay-diff.json
            echo
            echo '### Divergent entries'
            jq -r '.entries[] | select(.legacyVsNew | length > 0) | "- `\(.signature)` — \(.legacyVsNew | join("; "))"' replay-diff.json
          } >> "$GITHUB_STEP_SUMMARY"

      - name: Upload diff artifact
        uses: actions/upload-artifact@v4
        with:
          name: replay-diff
          path: replay-diff.json
```

Use a paid RPC (Helius / Triton / QuickNode) — the public mainnet endpoint
rate-limits enough that the replay step will fail intermittently against it.
Store the RPC URL as `MAINNET_RPC_URL` in repository secrets.

---

## Local equivalent

Everything above runs locally without GitHub. Reproduce any check with:

```bash
sta scan path/to/protocol --out manifest.json --sarif-out p-token.sarif --summary
replay --rpc https://your.rpc/ --program YourProgram... --limit 25 --out diff.json
```

`solana-token-analyzer` is intentionally local-first; the GitHub Action is
just the same binary running on a managed runner.
