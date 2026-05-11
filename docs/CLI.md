# CLI

The Rust CLI is the product surface for developers who want to run scans in local projects, CI, or migration scripts.

## Commands

```bash
npm run cli
```

Prints usage.

```bash
npm run cli -- interactive
```

Starts a persistent command session:

```text
p-token migrator interactive
Type /help for commands, /exit to quit.

p-token> /scan samples/anchor-token-vault
p-token> /bundle samples/anchor-token-vault data/migration-bundle
p-token> /validate data/migration-bundle/migration-manifest.json
p-token> /exit
```

Interactive commands:

- `/scan <project-path>`
- `/manifest <project-path> [manifest.json]`
- `/bundle <project-path> [bundle-dir]`
- `/validate [manifest.json]`
- `/wizard`
- `/last`
- `/exit`

```bash
npm run cli -- wizard
```

Starts an interactive guided scan. It asks for:

- project path
- manifest output path
- migration bundle directory
- whether to print a summary
- whether review-required findings should exit non-zero

```bash
npm run cli -- scan samples/anchor-token-vault --summary
```

Scans a project and prints a human-readable summary.

```bash
npm run cli -- scan samples/anchor-token-vault --out data/manifest.json
```

Writes the full migration manifest.

```bash
npm run cli -- scan samples/anchor-token-vault --bundle-out data/migration-bundle
```

Writes a migration bundle containing:

- `README.md`
- `migration-manifest.json`
- `patches/p-token-replacements.rs`
- `crates/p-token-shim-anchor/USAGE.md`

```bash
npm run cli -- scan samples/anchor-token-vault --sarif-out data/p-token.sarif
```

Writes SARIF for GitHub/code-scanning workflows.

```bash
npm run cli -- validate data/manifest.json
```

Checks that a manifest has the required top-level fields.

## CI Mode

Use `--fail-on-review` when a pipeline should fail if high-risk findings require manual review.

```bash
npm run cli -- scan samples/anchor-token-vault --summary --fail-on-review
npm run cli -- validate data/manifest.json --fail-on-review
```

GitHub Actions can upload SARIF after a scan:

```yaml
- run: npm run cli -- scan . --sarif-out p-token.sarif --summary
- uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: p-token.sarif
```

The bundled sample intentionally exits non-zero with `--fail-on-review` because it contains signer/token-program risk that should be reviewed.

## Expected Sample Output

The bundled sample should report:

- files scanned: `2`
- call sites: `3`
- operations: `transfer`, `mint_to`, `burn`
- simulation status: `review_required`
