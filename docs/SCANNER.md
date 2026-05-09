# Scanner

The hosted scanner is implemented in `src/migrator.ts`.

## Current Detection

The scanner accepts Rust, Anchor IDL JSON, and TOML files. For Rust files it:

- strips line comments, block comments, and string literals before matching;
- detects `anchor_spl::token` CPI calls such as `token::transfer(...)`;
- detects aliases such as `use anchor_spl::token as token_cpi;`;
- detects `spl_token::instruction` calls and aliases such as `use spl_token::instruction as token_ix;`;
- records file, line, operation, risk, CU estimate, and replacement guidance.

It intentionally counts CPI/instruction call sites, not account struct construction.

## Current Limits

This is still a lexical scanner, not a full Rust parser. It does not yet build an AST, resolve modules across crates, or prove that a matched function is the exact SPL Token call when user code shadows the same namespace.

## Next Parser Step

The next production-grade scanner should parse Rust syntax and resolve imports with a crate such as `syn` in a dedicated Rust scanning crate. The TypeScript scanner can remain as the upload/API coordinator while Rust owns source analysis.
