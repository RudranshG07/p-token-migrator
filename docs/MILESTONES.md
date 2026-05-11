# Milestones

## 1. IDL Scanner

Status: MVP complete.

The toolkit scans Rust, Anchor IDL JSON, and TOML files. It detects common `anchor_spl::token` and `spl_token::instruction` CPI call sites, strips comments and strings before matching, supports token module aliases, and emits a migration manifest.

## 2. p-token Codegen + CU Diff

Status: MVP complete.

Each scan emits replacement guidance, per-call-site CU estimates, aggregate savings, and a generated migration bundle through the API response. The bundle includes a migration plan, replacement snippets, and shim usage notes.

## 3. Forked-Mainnet Dry-Run Simulator

Status: MVP complete, forked-mainnet backend pending.

The current simulator is deterministic and flags review-required divergence risks from signer seeds, token-program accounts, checked operations, and IDL-only token program references. Real forked-mainnet replay requires finalized p-token program interfaces and a configured RPC/fork provider.

## 4. Compatibility Shim Anchor Crate

Status: MVP complete.

The repository includes `crates/p-token-shim-anchor`, a local transition shim scaffold. It gives generated migration code a stable routing boundary between legacy SPL Token and p-token. The internals should be replaced with canonical Anchor CPI wrappers when p-token mainnet interfaces are finalized.

## 5. Migration Dashboard + Public Launch

Status: MVP complete.

The project includes a React TSX dashboard, browser folder uploads, public report pages, Docker, CI, deployment docs, API docs, security notes, and storage docs.

## Remaining Production Work

- Replace lexical scanning with Rust AST and import resolution.
- Connect simulator mode to a forked-mainnet provider.
- Replace shim internals with final p-token instruction builders.
- Deploy with durable storage, HTTPS, monitoring, and a public domain.
