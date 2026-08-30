//! Compute-unit benchmark harness.
//!
//! Runs the canonical SPL Token (and, when wired up, Token-2022 / p-token)
//! instructions against a LiteSVM instance and records the exact compute-
//! units consumed by each. Emits a `BenchmarkProfile` JSON document in the
//! same shape `solana-token-analyzer` expects via `sta scan --profile-path`.
//!
//! Scope of this first cut:
//!   - Measure real CU for SPL Token `transfer`, `mint_to`, `burn`,
//!     `approve`, `close_account`, and `initialize_account`.
//!   - Token-2022 measurements are gated behind `--include-token-2022` and
//!     are not yet wired into the emitted profile shape (the profile only
//!     carries `legacyCu` / `pTokenCu` for now).
//!   - The p-token side stays as a placeholder ratio derived from the
//!     measured legacy CU until the canonical p-token program is available.

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use litesvm::LiteSVM;
use serde::Serialize;
use solana_sdk::{
    instruction::Instruction,
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};
use spl_token::{
    instruction as token_ix,
    state::{Account as TokenAccount, Mint},
};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Ratio applied to measured legacy SPL Token CU when generating placeholder
/// p-token estimates. SIMD-0266 targets 95–98% reduction; we use 0.04 (96%
/// reduction) as a deliberately conservative midpoint. Replaced with measured
/// p-token CU once the canonical program is available.
const PTOKEN_PLACEHOLDER_RATIO: f64 = 0.04;

const LAMPORTS_PER_SOL: u64 = 1_000_000_000;

#[derive(Parser, Debug)]
#[command(name = "cu-bench", about = "Measure SPL Token / Token-2022 CU costs via LiteSVM.")]
struct Cli {
    /// Write the resulting profile JSON to this path. Prints to stdout when omitted.
    #[arg(long)]
    out: Option<PathBuf>,
    /// Also measure Token-2022 instructions (currently informational only).
    #[arg(long)]
    include_token_2022: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OperationCu {
    legacy_cu: u64,
    p_token_cu: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BenchmarkProfile {
    name: String,
    status: String,
    note: String,
    operations: BTreeMap<String, OperationCu>,
    /// Provenance: token-2022 numbers when measured. Optional, ignored by sta
    /// scan today; reserved for the platform's token-2022 analysis surface.
    #[serde(skip_serializing_if = "Option::is_none")]
    token_2022: Option<BTreeMap<String, u64>>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let legacy = measure_spl_token().context("measuring SPL Token CU")?;
    let token_2022 = if cli.include_token_2022 {
        Some(measure_token_2022().context("measuring Token-2022 CU")?)
    } else {
        None
    };

    let operations: BTreeMap<String, OperationCu> = legacy
        .iter()
        .map(|(op, cu)| {
            (
                op.clone(),
                OperationCu {
                    legacy_cu: *cu,
                    p_token_cu: ((*cu as f64) * PTOKEN_PLACEHOLDER_RATIO).round() as u64,
                },
            )
        })
        .collect();

    let profile = BenchmarkProfile {
        name: "measured-litesvm".to_string(),
        status: "measured-legacy-placeholder-ptoken".to_string(),
        note: format!(
            "Legacy SPL Token CU values measured via LiteSVM. p-token values are a {:.0}% reduction placeholder until the canonical p-token program is available — replace by re-running cu-bench against p-token.",
            (1.0 - PTOKEN_PLACEHOLDER_RATIO) * 100.0
        ),
        operations,
        token_2022,
    };

    let json = serde_json::to_string_pretty(&profile)?;
    match cli.out {
        Some(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("creating {}", parent.display()))?;
                }
            }
            std::fs::write(&path, &json)
                .with_context(|| format!("writing {}", path.display()))?;
            eprintln!("wrote {}", path.display());
        }
        None => println!("{}", json),
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// SPL Token measurements
// ---------------------------------------------------------------------------

fn measure_spl_token() -> Result<BTreeMap<String, u64>> {
    let mut svm = LiteSVM::new();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 100 * LAMPORTS_PER_SOL)
        .map_err(|e| anyhow!("airdrop failed: {:?}", e))?;

    let mint_authority = Keypair::new();
    let owner = Keypair::new();
    let recipient_owner = Keypair::new();
    let delegate = Keypair::new();

    // --- create mint ---
    let mint = create_mint(&mut svm, &payer, &mint_authority.pubkey(), 6, &spl_token::ID)?;

    // --- create three accounts: source, destination, dest_for_close ---
    let source = create_token_account(&mut svm, &payer, &mint, &owner.pubkey(), &spl_token::ID)?;
    let destination =
        create_token_account(&mut svm, &payer, &mint, &recipient_owner.pubkey(), &spl_token::ID)?;
    let _close_target =
        create_token_account(&mut svm, &payer, &mint, &owner.pubkey(), &spl_token::ID)?;

    let mint_to_ix = token_ix::mint_to(
        &spl_token::ID,
        &mint,
        &source,
        &mint_authority.pubkey(),
        &[],
        1_000_000,
    )?;
    let mint_to_cu = measure(&mut svm, &payer, "mint_to", mint_to_ix, &[&mint_authority])?;

    let transfer_ix = token_ix::transfer(
        &spl_token::ID,
        &source,
        &destination,
        &owner.pubkey(),
        &[],
        1_000,
    )?;
    let transfer_cu = measure(&mut svm, &payer, "transfer", transfer_ix, &[&owner])?;

    let approve_ix = token_ix::approve(
        &spl_token::ID,
        &source,
        &delegate.pubkey(),
        &owner.pubkey(),
        &[],
        500,
    )?;
    let approve_cu = measure(&mut svm, &payer, "approve", approve_ix, &[&owner])?;

    let burn_ix = token_ix::burn(
        &spl_token::ID,
        &source,
        &mint,
        &owner.pubkey(),
        &[],
        100,
    )?;
    let burn_cu = measure(&mut svm, &payer, "burn", burn_ix, &[&owner])?;

    // close_account: drain destination back to source, then close.
    let drain = token_ix::transfer(
        &spl_token::ID,
        &destination,
        &source,
        &recipient_owner.pubkey(),
        &[],
        1_000,
    )?;
    let _ = run_one(&mut svm, &payer, drain, &[&recipient_owner])?;
    let close_ix = token_ix::close_account(
        &spl_token::ID,
        &destination,
        &owner.pubkey(),
        &recipient_owner.pubkey(),
        &[],
    )?;
    let close_cu = measure(&mut svm, &payer, "close_account", close_ix, &[&recipient_owner])?;

    // initialize_account: pre-allocate, then measure only the init step.
    let new_account = Keypair::new();
    let rent = svm.minimum_balance_for_rent_exemption(TokenAccount::LEN);
    let create = system_instruction::create_account(
        &payer.pubkey(),
        &new_account.pubkey(),
        rent,
        TokenAccount::LEN as u64,
        &spl_token::ID,
    );
    let _ = run_one(&mut svm, &payer, create, &[&new_account])?;
    let init = token_ix::initialize_account(
        &spl_token::ID,
        &new_account.pubkey(),
        &mint,
        &owner.pubkey(),
    )?;
    let init_account_cu = measure(&mut svm, &payer, "initialize_account", init, &[])?;

    let mut out = BTreeMap::new();
    out.insert("transfer".to_string(), transfer_cu);
    out.insert("mint_to".to_string(), mint_to_cu);
    out.insert("burn".to_string(), burn_cu);
    out.insert("approve".to_string(), approve_cu);
    out.insert("close_account".to_string(), close_cu);
    out.insert("initialize_account".to_string(), init_account_cu);
    Ok(out)
}

// ---------------------------------------------------------------------------
// Token-2022 measurements (no extensions). Recorded but not yet wired into
// the consumed profile shape — kept for the platform's token-2022 surface.
// ---------------------------------------------------------------------------

fn measure_token_2022() -> Result<BTreeMap<String, u64>> {
    let mut svm = LiteSVM::new();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 100 * LAMPORTS_PER_SOL)
        .map_err(|e| anyhow!("airdrop failed: {:?}", e))?;

    let mint_authority = Keypair::new();
    let owner = Keypair::new();
    let recipient_owner = Keypair::new();

    let program_id = spl_token_2022::ID;
    let mint = create_mint(&mut svm, &payer, &mint_authority.pubkey(), 6, &program_id)?;
    let source = create_token_account(&mut svm, &payer, &mint, &owner.pubkey(), &program_id)?;
    let destination =
        create_token_account(&mut svm, &payer, &mint, &recipient_owner.pubkey(), &program_id)?;

    let mint_to_ix = spl_token_2022::instruction::mint_to(
        &program_id,
        &mint,
        &source,
        &mint_authority.pubkey(),
        &[],
        1_000_000,
    )?;
    let mint_to_cu = measure(&mut svm, &payer, "t22 mint_to", mint_to_ix, &[&mint_authority])?;

    let transfer_ix = spl_token_2022::instruction::transfer_checked(
        &program_id,
        &source,
        &mint,
        &destination,
        &owner.pubkey(),
        &[],
        1_000,
        6,
    )?;
    let transfer_cu = measure(&mut svm, &payer, "t22 transfer_checked", transfer_ix, &[&owner])?;

    let mut out = BTreeMap::new();
    out.insert("transfer".to_string(), transfer_cu);
    out.insert("mint_to".to_string(), mint_to_cu);
    Ok(out)
}

// ---------------------------------------------------------------------------
// LiteSVM helpers
// ---------------------------------------------------------------------------

fn create_mint(
    svm: &mut LiteSVM,
    payer: &Keypair,
    mint_authority: &Pubkey,
    decimals: u8,
    program_id: &Pubkey,
) -> Result<Pubkey> {
    let mint = Keypair::new();
    let rent = svm.minimum_balance_for_rent_exemption(Mint::LEN);
    let create = system_instruction::create_account(
        &payer.pubkey(),
        &mint.pubkey(),
        rent,
        Mint::LEN as u64,
        program_id,
    );
    let init = if program_id == &spl_token::ID {
        token_ix::initialize_mint(program_id, &mint.pubkey(), mint_authority, None, decimals)?
    } else {
        spl_token_2022::instruction::initialize_mint(
            program_id,
            &mint.pubkey(),
            mint_authority,
            None,
            decimals,
        )?
    };
    send(svm, payer, &[create, init], &[&mint])?;
    Ok(mint.pubkey())
}

fn create_token_account(
    svm: &mut LiteSVM,
    payer: &Keypair,
    mint: &Pubkey,
    owner: &Pubkey,
    program_id: &Pubkey,
) -> Result<Pubkey> {
    let account = Keypair::new();
    let rent = svm.minimum_balance_for_rent_exemption(TokenAccount::LEN);
    let create = system_instruction::create_account(
        &payer.pubkey(),
        &account.pubkey(),
        rent,
        TokenAccount::LEN as u64,
        program_id,
    );
    let init = if program_id == &spl_token::ID {
        token_ix::initialize_account(program_id, &account.pubkey(), mint, owner)?
    } else {
        spl_token_2022::instruction::initialize_account(program_id, &account.pubkey(), mint, owner)?
    };
    send(svm, payer, &[create, init], &[&account])?;
    Ok(account.pubkey())
}

fn measure(
    svm: &mut LiteSVM,
    payer: &Keypair,
    label: &str,
    ix: Instruction,
    extra_signers: &[&Keypair],
) -> Result<u64> {
    let cu = run_one(svm, payer, ix, extra_signers)?;
    eprintln!("  {label:<22} {cu} CU");
    Ok(cu)
}

fn run_one(
    svm: &mut LiteSVM,
    payer: &Keypair,
    ix: Instruction,
    extra_signers: &[&Keypair],
) -> Result<u64> {
    let mut signers: Vec<&Keypair> = vec![payer];
    signers.extend_from_slice(extra_signers);
    let blockhash = svm.latest_blockhash();
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &signers,
        blockhash,
    );
    let meta = svm
        .send_transaction(tx)
        .map_err(|e| anyhow!("transaction failed: {:?}", e))?;
    Ok(meta.compute_units_consumed)
}

fn send(
    svm: &mut LiteSVM,
    payer: &Keypair,
    ixs: &[Instruction],
    extra_signers: &[&Keypair],
) -> Result<()> {
    let mut signers: Vec<&Keypair> = vec![payer];
    signers.extend_from_slice(extra_signers);
    let blockhash = svm.latest_blockhash();
    let tx = Transaction::new_signed_with_payer(ixs, Some(&payer.pubkey()), &signers, blockhash);
    svm.send_transaction(tx)
        .map(|_| ())
        .map_err(|e| anyhow!("setup transaction failed: {:?}", e))
}
