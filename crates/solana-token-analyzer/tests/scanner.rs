use solana_token_analyzer::{
    scan_project, scan_source_files, Operation, PatternKind, RiskLevel, ScanOptions, SourceFile,
    TokenProgram,
};
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn scans_sample_anchor_vault() {
    let sample = workspace_root().join("samples/anchor-token-vault");
    let manifest = scan_project(&sample, ScanOptions::default()).expect("scan ok");

    assert_eq!(manifest.protocol, "Anchor Token Vault");
    assert_eq!(manifest.totals.call_sites, 3);
    assert_eq!(manifest.totals.files_scanned, 2);
    assert_eq!(manifest.idl_hints.len(), 1);

    let ops: Vec<Operation> = manifest.findings.iter().map(|f| f.operation).collect();
    assert!(ops.contains(&Operation::Transfer));
    assert!(ops.contains(&Operation::MintTo));
    assert!(ops.contains(&Operation::Burn));

    for f in &manifest.findings {
        assert_eq!(f.token_program, TokenProgram::SplToken);
        assert_eq!(f.pattern_kind, PatternKind::AnchorCpi);
        assert!(f.line > 0);
        assert!(!f.snippet.is_empty());
    }
}

#[test]
fn detects_aliased_anchor_token_imports() {
    let source = SourceFile {
        relative: "src/lib.rs".to_string(),
        content: r#"
use anchor_lang::prelude::*;
use anchor_spl::token as t;

pub fn run(ctx: CpiContext<t::Transfer>, amount: u64) -> Result<()> {
    t::transfer(ctx, amount)?;
    Ok(())
}
"#
        .to_string(),
    };
    let m = scan_source_files(vec![source], ScanOptions::default()).expect("scan ok");
    assert_eq!(m.totals.call_sites, 1, "alias `t::transfer` must be detected");
    let f = &m.findings[0];
    assert_eq!(f.operation, Operation::Transfer);
    assert_eq!(f.token_program, TokenProgram::SplToken);
    assert_eq!(f.pattern_kind, PatternKind::AnchorCpi);
}

#[test]
fn detects_self_group_import() {
    let source = SourceFile {
        relative: "src/lib.rs".to_string(),
        content: r#"
use anchor_spl::token::{self, Transfer, MintTo};

pub fn run() {
    token::transfer(unimplemented!(), 1).unwrap();
    token::mint_to(unimplemented!(), 1).unwrap();
}
"#
        .to_string(),
    };
    let m = scan_source_files(vec![source], ScanOptions::default()).expect("scan ok");
    assert_eq!(m.totals.call_sites, 2);
}

#[test]
fn detects_token_interface_and_marks_high_risk() {
    let source = SourceFile {
        relative: "src/lib.rs".to_string(),
        content: r#"
use anchor_spl::token_interface;

pub fn run() {
    token_interface::transfer_checked(unimplemented!(), 1, 6).unwrap();
}
"#
        .to_string(),
    };
    let m = scan_source_files(vec![source], ScanOptions::default()).expect("scan ok");
    assert_eq!(m.totals.call_sites, 1);
    let f = &m.findings[0];
    assert_eq!(f.token_program, TokenProgram::TokenInterface);
    assert_eq!(f.pattern_kind, PatternKind::TokenInterface);
    assert_eq!(
        f.risk.level,
        RiskLevel::High,
        "token_interface (token-2022-aware) must be flagged high"
    );
}

#[test]
fn detects_spl_token_2022_instruction() {
    let source = SourceFile {
        relative: "src/lib.rs".to_string(),
        content: r#"
use spl_token_2022::instruction;

pub fn run() {
    let _ix = instruction::transfer_checked(
        unimplemented!(), unimplemented!(), unimplemented!(),
        unimplemented!(), &[], 1, 6,
    ).unwrap();
}
"#
        .to_string(),
    };
    let m = scan_source_files(vec![source], ScanOptions::default()).expect("scan ok");
    assert_eq!(m.totals.call_sites, 1);
    let f = &m.findings[0];
    assert_eq!(f.token_program, TokenProgram::SplToken2022);
    assert_eq!(f.pattern_kind, PatternKind::SplInstruction);
}

#[test]
fn ignores_token_calls_in_comments_and_strings() {
    // The regex-based scanner needed a separate sanitizer to strip these.
    // The AST scanner gets this for free — parse trees don't contain
    // comment or string-literal call sites.
    let source = SourceFile {
        relative: "src/lib.rs".to_string(),
        content: r#"
use anchor_spl::token;

// token::transfer(stuff, 1)?; -- this is in a comment
/// Doc: call token::mint_to(ctx, amount) somewhere
pub fn run() {
    let _s = "token::burn(this is a string, not a call)";
    /* token::approve(also a comment, 1)?; */
    token::transfer(unimplemented!(), 1).unwrap();
}
"#
        .to_string(),
    };
    let m = scan_source_files(vec![source], ScanOptions::default()).expect("scan ok");
    assert_eq!(
        m.totals.call_sites, 1,
        "only the real call site should be detected, not the comments or string"
    );
    assert_eq!(m.findings[0].operation, Operation::Transfer);
}

#[test]
fn detects_full_path_call_without_use_statement() {
    let source = SourceFile {
        relative: "src/lib.rs".to_string(),
        content: r#"
pub fn run() {
    anchor_spl::token::transfer(unimplemented!(), 1).unwrap();
    spl_token::instruction::burn(
        unimplemented!(), unimplemented!(), unimplemented!(),
        unimplemented!(), &[], 1,
    ).unwrap();
}
"#
        .to_string(),
    };
    let m = scan_source_files(vec![source], ScanOptions::default()).expect("scan ok");
    assert_eq!(m.totals.call_sites, 2);
    let by_kind: Vec<PatternKind> = m.findings.iter().map(|f| f.pattern_kind).collect();
    assert!(by_kind.contains(&PatternKind::AnchorCpi));
    assert!(by_kind.contains(&PatternKind::SplInstruction));
}

#[test]
fn detects_same_file_helper_wrapper() {
    let source = SourceFile {
        relative: "src/lib.rs".to_string(),
        content: r#"
use anchor_spl::token;

pub fn safe_transfer(amount: u64) {
    token::transfer(unimplemented!(), amount).unwrap();
}

pub fn handle_swap(amount: u64) {
    safe_transfer(amount);
    safe_transfer(amount + 1);
}

pub fn handle_withdraw(amount: u64) {
    safe_transfer(amount);
}
"#
        .to_string(),
    };
    let m = scan_source_files(vec![source], ScanOptions::default()).expect("scan ok");

    // 1 direct hit inside safe_transfer + 3 wrapper calls = 4 findings.
    assert_eq!(m.totals.call_sites, 4, "got findings: {:#?}", m.findings);

    let direct: Vec<&_> = m
        .findings
        .iter()
        .filter(|f| f.via_wrapper.is_none())
        .collect();
    let wrapped: Vec<&_> = m
        .findings
        .iter()
        .filter(|f| f.via_wrapper.is_some())
        .collect();
    assert_eq!(direct.len(), 1);
    assert_eq!(direct[0].pattern_kind, PatternKind::AnchorCpi);
    assert_eq!(wrapped.len(), 3);
    for w in &wrapped {
        assert_eq!(w.pattern_kind, PatternKind::WrappedCpi);
        assert_eq!(w.via_wrapper.as_deref(), Some("safe_transfer"));
        assert_eq!(w.operation, Operation::Transfer);
        assert_eq!(w.confidence, replay_test_helpers::medium_confidence());
    }
}

#[test]
fn detects_cross_file_helper_wrapper() {
    let utils = SourceFile {
        relative: "src/utils.rs".to_string(),
        content: r#"
use anchor_spl::token;
pub fn safe_burn(amount: u64) {
    token::burn(unimplemented!(), amount).unwrap();
}
"#
        .to_string(),
    };
    let handlers = SourceFile {
        relative: "src/handlers.rs".to_string(),
        content: r#"
use crate::utils::safe_burn;
pub fn redeem(amount: u64) {
    safe_burn(amount);
}
pub fn liquidate(amount: u64) {
    safe_burn(amount);
}
"#
        .to_string(),
    };
    let m = scan_source_files(vec![utils, handlers], ScanOptions::default()).expect("scan ok");

    let wrapped_in_handlers: Vec<&_> = m
        .findings
        .iter()
        .filter(|f| f.file == "src/handlers.rs")
        .collect();
    assert_eq!(wrapped_in_handlers.len(), 2, "got: {wrapped_in_handlers:#?}");
    for w in &wrapped_in_handlers {
        assert_eq!(w.via_wrapper.as_deref(), Some("safe_burn"));
        assert_eq!(w.operation, Operation::Burn);
    }
}

#[test]
fn transitive_wrapper_through_two_levels() {
    let source = SourceFile {
        relative: "src/lib.rs".to_string(),
        content: r#"
use anchor_spl::token;
pub fn inner(amount: u64) {
    token::mint_to(unimplemented!(), amount).unwrap();
}
pub fn middle(amount: u64) {
    inner(amount)
}
pub fn outer(amount: u64) {
    middle(amount)
}
pub fn handler(amount: u64) {
    outer(amount);
}
"#
        .to_string(),
    };
    let m = scan_source_files(vec![source], ScanOptions::default()).expect("scan ok");

    // The handler's call to outer() should resolve to a mint_to wrapper hit
    // even though outer() is two levels removed from the direct CPI.
    let handler_finding = m
        .findings
        .iter()
        .find(|f| f.via_wrapper.as_deref() == Some("outer"));
    assert!(
        handler_finding.is_some(),
        "expected an `outer`-wrapped finding; got: {:#?}",
        m.findings
    );
    assert_eq!(handler_finding.unwrap().operation, Operation::MintTo);
}

#[test]
fn detects_spl_token_2022_onchain_invoke_transfer_checked() {
    // Real-world pattern from marginfi-v2 state/bank.rs:660,707.
    let source = SourceFile {
        relative: "src/lib.rs".to_string(),
        content: r#"
pub fn do_transfer() {
    spl_token_2022::onchain::invoke_transfer_checked(
        program_id, from, mint, to, authority, remaining, amount, decimals, &[],
    ).unwrap();
}
"#
        .to_string(),
    };
    let m = scan_source_files(vec![source], ScanOptions::default()).expect("scan ok");
    assert_eq!(m.totals.call_sites, 1);
    let f = &m.findings[0];
    assert_eq!(f.operation, Operation::Transfer);
    assert_eq!(f.token_program, TokenProgram::SplToken2022);
    assert_eq!(f.pattern_kind, PatternKind::SplInstruction);
}

#[test]
fn detects_impl_method_wrapper_called_via_self() {
    let source = SourceFile {
        relative: "src/lib.rs".to_string(),
        content: r#"
use anchor_spl::token;

pub struct Vault;

impl Vault {
    pub fn safe_transfer(&self, amount: u64) {
        token::transfer(unimplemented!(), amount).unwrap();
    }

    pub fn handle_deposit(&self, amount: u64) {
        self.safe_transfer(amount);
    }
}

pub fn external(vault: &Vault, amount: u64) {
    vault.safe_transfer(amount);
}
"#
        .to_string(),
    };
    let m = scan_source_files(vec![source], ScanOptions::default()).expect("scan ok");

    let direct: Vec<&_> = m
        .findings
        .iter()
        .filter(|f| f.via_wrapper.is_none())
        .collect();
    let wrapped: Vec<&_> = m
        .findings
        .iter()
        .filter(|f| f.via_wrapper.is_some())
        .collect();

    assert_eq!(direct.len(), 1, "expected one direct CPI in the method body");
    assert_eq!(
        wrapped.len(),
        2,
        "expected wrapper findings for self.safe_transfer and vault.safe_transfer; got: {wrapped:#?}"
    );
    for w in &wrapped {
        assert_eq!(w.via_wrapper.as_deref(), Some("safe_transfer"));
        assert_eq!(w.operation, Operation::Transfer);
        assert_eq!(w.pattern_kind, PatternKind::WrappedCpi);
    }
}

#[test]
fn wrapper_with_same_name_as_token_op_does_not_double_count() {
    // Edge case: a function literally named `transfer` exists, but the
    // real `token::transfer(..)` call shouldn't also emit a wrapper finding.
    let source = SourceFile {
        relative: "src/lib.rs".to_string(),
        content: r#"
use anchor_spl::token;
pub fn transfer(amount: u64) {
    token::transfer(unimplemented!(), amount).unwrap();
}
"#
        .to_string(),
    };
    let m = scan_source_files(vec![source], ScanOptions::default()).expect("scan ok");
    // Exactly one direct hit. No wrapper finding even though `transfer` is
    // also a function name in this scope.
    let on_line: Vec<_> = m
        .findings
        .iter()
        .filter(|f| f.line == 4)
        .collect();
    assert_eq!(on_line.len(), 1);
    assert!(on_line[0].via_wrapper.is_none());
}

// Tiny helper module so the test asserting Confidence::Medium doesn't have
// to import the enum at the top of the test file.
mod replay_test_helpers {
    use solana_token_analyzer::Confidence;
    pub fn medium_confidence() -> Confidence {
        Confidence::Medium
    }
}

#[test]
fn handles_unparseable_rust_gracefully() {
    let source = SourceFile {
        relative: "src/broken.rs".to_string(),
        content: "this is not valid rust @@@".to_string(),
    };
    let m = scan_source_files(vec![source], ScanOptions::default()).expect("scan must not panic");
    assert_eq!(m.totals.call_sites, 0);
    assert_eq!(m.totals.files_scanned, 1);
}
