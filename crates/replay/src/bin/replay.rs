use anyhow::{anyhow, Context, Result};
use clap::Parser;
use replay::{
    compare_builds, replay_signature, BuildDiffReport, ReplayOptions, ReplayReport, RpcClient,
};
use solana_sdk::pubkey::Pubkey;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Parser, Debug)]
#[command(
    name = "replay",
    about = "Replay Solana mainnet transactions against forked state. With --legacy-so and --new-so, replays each tx twice against the two builds and diffs the outcome."
)]
struct Cli {
    /// JSON-RPC endpoint (paid provider strongly recommended — public RPC
    /// rate-limits aggressively).
    #[arg(long)]
    rpc: String,
    /// Program address whose recent transactions should be replayed.
    #[arg(long)]
    program: String,
    /// Number of recent signatures to fetch and replay.
    #[arg(long, default_value_t = 5)]
    limit: usize,
    /// Optional output path. Prints JSON to stdout when omitted.
    #[arg(long)]
    out: Option<PathBuf>,
    /// Path to the *legacy* program ELF. When supplied together with
    /// `--new-so`, switches to diff mode: each tx is replayed against both
    /// builds and the outcomes are compared.
    #[arg(long)]
    legacy_so: Option<PathBuf>,
    /// Path to the *new* program ELF. See `--legacy-so`.
    #[arg(long)]
    new_so: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let program = Pubkey::from_str(&cli.program)
        .with_context(|| format!("parsing program pubkey {}", cli.program))?;
    let rpc = RpcClient::http(cli.rpc.clone());

    match (&cli.legacy_so, &cli.new_so) {
        (Some(legacy_path), Some(new_path)) => {
            run_diff(&rpc, &cli, &program, legacy_path, new_path)
        }
        (None, None) => run_single(&rpc, &cli, &program),
        _ => Err(anyhow!(
            "--legacy-so and --new-so must be supplied together (diff mode), or neither (single-build replay mode)"
        )),
    }
}

fn run_single(rpc: &RpcClient, cli: &Cli, program: &Pubkey) -> Result<()> {
    let sigs = rpc.get_signatures_for_address(program, cli.limit)?;
    eprintln!("fetched {} recent signatures for {}", sigs.len(), program);

    let opts = ReplayOptions::default();
    let mut outcomes = Vec::with_capacity(sigs.len());
    for (i, sig) in sigs.iter().enumerate() {
        eprintln!(
            "  [{}/{}] replaying {} (slot {})",
            i + 1,
            sigs.len(),
            sig.signature,
            sig.slot
        );
        match replay_signature(rpc, &sig.signature, &opts) {
            Ok(outcome) => outcomes.push(outcome),
            Err(e) => eprintln!("    error: {e:?}"),
        }
    }

    let report = ReplayReport::from_outcomes(cli.program.clone(), cli.rpc.clone(), outcomes);
    emit(
        &serde_json::to_string_pretty(&report)?,
        cli.out.as_deref(),
        &format!(
            "{} matched, {} diverged, {} skipped",
            report.matched, report.diverged, report.skipped
        ),
    )
}

fn run_diff(
    rpc: &RpcClient,
    cli: &Cli,
    program: &Pubkey,
    legacy_path: &std::path::Path,
    new_path: &std::path::Path,
) -> Result<()> {
    let legacy_elf =
        fs::read(legacy_path).with_context(|| format!("reading {}", legacy_path.display()))?;
    let new_elf =
        fs::read(new_path).with_context(|| format!("reading {}", new_path.display()))?;
    eprintln!(
        "diff mode: legacy={} bytes, new={} bytes — target program {}",
        legacy_elf.len(),
        new_elf.len(),
        program
    );

    let sigs = rpc.get_signatures_for_address(program, cli.limit)?;
    eprintln!("fetched {} recent signatures", sigs.len());

    let opts = ReplayOptions::default();
    let mut outcomes = Vec::with_capacity(sigs.len());
    for (i, sig) in sigs.iter().enumerate() {
        eprintln!(
            "  [{}/{}] diffing {} (slot {})",
            i + 1,
            sigs.len(),
            sig.signature,
            sig.slot
        );
        match compare_builds(rpc, &sig.signature, program, &legacy_elf, &new_elf, &opts) {
            Ok(outcome) => outcomes.push(outcome),
            Err(e) => eprintln!("    error: {e:?}"),
        }
    }

    let report = BuildDiffReport::from_outcomes(
        cli.program.clone(),
        legacy_path.display().to_string(),
        new_path.display().to_string(),
        cli.rpc.clone(),
        outcomes,
    );
    emit(
        &serde_json::to_string_pretty(&report)?,
        cli.out.as_deref(),
        &format!(
            "{} matched, {} diverged, {} skipped",
            report.matched, report.diverged, report.skipped
        ),
    )
}

fn emit(json: &str, out: Option<&std::path::Path>, summary: &str) -> Result<()> {
    match out {
        Some(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("creating {}", parent.display()))?;
                }
            }
            fs::write(path, json).with_context(|| format!("writing {}", path.display()))?;
            eprintln!("\nwrote {} ({})", path.display(), summary);
        }
        None => println!("{}", json),
    }
    Ok(())
}
