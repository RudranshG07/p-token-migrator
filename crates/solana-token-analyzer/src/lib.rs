//! AST-driven analyzer for Solana programs that use the SPL Token, Token-2022,
//! and p-token programs.
//!
//! The scanner parses Rust source with `syn`, resolves token-program aliases
//! from `use` statements, walks call expressions, and emits a manifest that is
//! wire-compatible with the existing TypeScript scanner's JSON shape (with one
//! additive field, `tokenProgram`, on each finding).

pub mod manifest;
pub mod profile;
pub mod risk;
pub mod sarif;
pub mod scanner;
pub mod simulation;

pub use manifest::*;
pub use profile::{BenchmarkProfile, default_profile};
pub use sarif::manifest_to_sarif;
pub use scanner::{scan_project, scan_source_files, SourceFile, ScanOptions};
pub use simulation::run_dry_run;
