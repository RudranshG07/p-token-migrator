use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use solana_token_analyzer::{
    manifest_to_sarif, scan_project, scan_source_files, BenchmarkProfile, Manifest, ScanOptions,
    SourceFile,
};
use std::fs;
use std::io::Read;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "sta",
    about = "Solana token-program static analyzer (SPL Token, Token-2022, p-token).",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Scan a project directory and emit a manifest.
    Scan {
        /// Path to the project root to scan.
        path: PathBuf,
        /// Optional protocol name. Defaults to the directory name, title-cased.
        #[arg(long)]
        protocol: Option<String>,
        /// Path to a benchmark profile JSON. Defaults to the built-in profile.
        #[arg(long)]
        profile_path: Option<PathBuf>,
        /// Write the manifest JSON to this path. Prints to stdout when omitted.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Also write a SARIF 2.1.0 report to this path for GitHub
        /// code-scanning ingestion.
        #[arg(long)]
        sarif_out: Option<PathBuf>,
        /// Print a short human-readable summary on stderr after the scan.
        #[arg(long)]
        summary: bool,
        /// Exit with a non-zero status code if any finding is flagged for review.
        #[arg(long)]
        fail_on_review: bool,
    },
    /// Scan in-memory source files supplied as JSON on stdin and emit the
    /// manifest on stdout. Used by the dashboard server to forward uploads
    /// to the engine without writing them to disk.
    ///
    /// Stdin JSON shape: `{ "protocol"?: string, "files": [{ "relative": ..., "content": ... }], "profile"?: BenchmarkProfile }`
    ScanSources,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Scan {
            path,
            protocol,
            profile_path,
            out,
            sarif_out,
            summary,
            fail_on_review,
        } => run_scan(path, protocol, profile_path, out, sarif_out, summary, fail_on_review),
        Command::ScanSources => run_scan_sources(),
    }
}

fn run_scan(
    path: PathBuf,
    protocol: Option<String>,
    profile_path: Option<PathBuf>,
    out: Option<PathBuf>,
    sarif_out: Option<PathBuf>,
    summary: bool,
    fail_on_review: bool,
) -> Result<()> {
    let profile = match profile_path {
        Some(p) => Some(load_profile(&p)?),
        None => None,
    };
    let manifest = scan_project(
        &path,
        ScanOptions {
            protocol,
            profile,
            ..Default::default()
        },
    )
    .with_context(|| format!("scanning {}", path.display()))?;

    emit_manifest(&manifest, out.as_deref())?;

    if let Some(sarif_path) = sarif_out {
        let sarif = manifest_to_sarif(&manifest);
        let json = serde_json::to_string_pretty(&sarif).context("serializing SARIF")?;
        write_to(&sarif_path, &json)?;
    }

    if summary {
        print_summary(&manifest);
    }

    if fail_on_review
        && matches!(
            manifest.simulation.status,
            solana_token_analyzer::SimulationStatus::ReviewRequired
        )
    {
        std::process::exit(2);
    }
    Ok(())
}

fn write_to(path: &std::path::Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
    }
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[derive(Deserialize)]
struct ScanSourcesStdin {
    #[serde(default)]
    protocol: Option<String>,
    #[serde(default)]
    files: Vec<SourceFileInput>,
    #[serde(default)]
    profile: Option<BenchmarkProfile>,
    #[serde(default)]
    root: Option<String>,
}

#[derive(Deserialize)]
struct SourceFileInput {
    relative: String,
    content: String,
}

fn run_scan_sources() -> Result<()> {
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .context("reading stdin")?;
    let payload: ScanSourcesStdin =
        serde_json::from_str(&buf).context("parsing stdin JSON for scan-sources")?;
    let files: Vec<SourceFile> = payload
        .files
        .into_iter()
        .map(|f| SourceFile {
            relative: f.relative,
            content: f.content,
        })
        .collect();
    let manifest = scan_source_files(
        files,
        ScanOptions {
            protocol: payload.protocol,
            profile: payload.profile,
            root: Some(payload.root.unwrap_or_else(|| "uploaded-sources".to_string())),
        },
    )
    .context("scanning uploaded sources")?;
    emit_manifest(&manifest, None)?;
    Ok(())
}

fn emit_manifest(manifest: &Manifest, out: Option<&std::path::Path>) -> Result<()> {
    let json = serde_json::to_string_pretty(manifest).context("serializing manifest")?;
    match out {
        Some(out_path) => {
            if let Some(parent) = out_path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("creating {}", parent.display()))?;
                }
            }
            fs::write(out_path, &json)
                .with_context(|| format!("writing {}", out_path.display()))?;
        }
        None => println!("{}", json),
    }
    Ok(())
}

fn load_profile(path: &std::path::Path) -> Result<BenchmarkProfile> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("reading profile {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parsing profile {}", path.display()))
}

fn print_summary(manifest: &Manifest) {
    eprintln!();
    eprintln!("Protocol:        {}", manifest.protocol);
    eprintln!("Files scanned:   {}", manifest.totals.files_scanned);
    eprintln!("Call sites:      {}", manifest.totals.call_sites);
    eprintln!(
        "Legacy CU:       {}  ->  p-token CU: {}  (saved {}, {}%)",
        manifest.totals.legacy_cu,
        manifest.totals.p_token_cu,
        manifest.totals.saved_cu,
        manifest.totals.savings_percent
    );
    eprintln!(
        "Simulation:      {:?} ({} review items)",
        manifest.simulation.status,
        manifest.simulation.divergences.len()
    );
    if !manifest.idl_hints.is_empty() {
        eprintln!("IDL hints:");
        for hint in &manifest.idl_hints {
            eprintln!("  - {} ({})", hint.file, hint.kind);
        }
    }
}
