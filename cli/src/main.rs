use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct Pattern {
    op: &'static str,
    needles: &'static [&'static str],
    legacy_cu: u64,
    p_token_cu: u64,
}

#[derive(Debug, Clone)]
struct Finding {
    file: String,
    line: usize,
    operation: &'static str,
    snippet: String,
    legacy_cu: u64,
    p_token_cu: u64,
    risk: &'static str,
    risk_reason: &'static str,
}

const PATTERNS: &[Pattern] = &[
    Pattern { op: "transfer", needles: &["token::transfer", "transfer_checked", "spl_token::instruction::transfer"], legacy_cu: 5200, p_token_cu: 220 },
    Pattern { op: "mint_to", needles: &["token::mint_to", "spl_token::instruction::mint_to"], legacy_cu: 6100, p_token_cu: 260 },
    Pattern { op: "burn", needles: &["token::burn", "spl_token::instruction::burn"], legacy_cu: 5700, p_token_cu: 250 },
    Pattern { op: "approve", needles: &["token::approve", "spl_token::instruction::approve"], legacy_cu: 4800, p_token_cu: 210 },
    Pattern { op: "close_account", needles: &["token::close_account", "CloseAccount {", "spl_token::instruction::close_account"], legacy_cu: 5000, p_token_cu: 240 },
    Pattern { op: "initialize_account", needles: &["InitializeAccount", "initialize_account", "spl_token::instruction::initialize_account"], legacy_cu: 7400, p_token_cu: 360 },
];

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 || args[1] != "scan" {
        print_usage();
        return Ok(());
    }

    let root = PathBuf::from(&args[2]);
    let out = parse_out_path(&args);
    let manifest = scan_project(&root)?;

    if let Some(out_path) = out {
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&out_path, &manifest)?;
        println!("manifest written to {}", out_path.display());
    } else {
        println!("{manifest}");
    }

    Ok(())
}

fn print_usage() {
    println!("p-token-migrator-cli");
    println!("usage: p-token-migrator-cli scan <project-path> [--out manifest.json]");
}

fn parse_out_path(args: &[String]) -> Option<PathBuf> {
    args.windows(2)
        .find(|pair| pair[0] == "--out")
        .map(|pair| PathBuf::from(&pair[1]))
}

fn scan_project(root: &Path) -> io::Result<String> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;

    let mut findings = Vec::new();
    let mut idl_hints = Vec::new();

    for file in &files {
        let source = fs::read_to_string(&file.absolute)?;
        if file.relative.ends_with(".json")
            && (source.contains("\"tokenProgram\"") || source.contains("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"))
        {
            idl_hints.push(file.relative.clone());
        }

        for (line_index, line) in source.lines().enumerate() {
            for pattern in PATTERNS {
                if pattern.needles.iter().any(|needle| line.contains(needle)) {
                    let (risk, risk_reason) = classify_risk(&source, line);
                    findings.push(Finding {
                        file: file.relative.clone(),
                        line: line_index + 1,
                        operation: pattern.op,
                        snippet: line.trim().to_string(),
                        legacy_cu: pattern.legacy_cu,
                        p_token_cu: pattern.p_token_cu,
                        risk,
                        risk_reason,
                    });
                }
            }
        }
    }

    Ok(render_manifest(root, files.len(), &findings, &idl_hints))
}

#[derive(Debug)]
struct ProjectFile {
    absolute: PathBuf,
    relative: String,
}

fn collect_files(root: &Path, current: &Path, files: &mut Vec<ProjectFile>) -> io::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if matches!(name.as_ref(), "node_modules" | ".git" | "dist" | ".next" | "target") {
                continue;
            }
            collect_files(root, &path, files)?;
        } else if matches!(path.extension().and_then(|ext| ext.to_str()), Some("rs" | "json" | "toml")) {
            let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();
            files.push(ProjectFile { absolute: path, relative });
        }
    }
    Ok(())
}

fn classify_risk(source: &str, snippet: &str) -> (&'static str, &'static str) {
    let lower = source.to_lowercase();
    let has_signer = lower.contains("signer") || lower.contains("with_signer") || lower.contains("seeds");
    let has_token_program = lower.contains("token_program");
    if has_signer && has_token_program {
        ("high", "Signer seeds and token program account constraints are present.")
    } else if has_token_program {
        ("medium", "Token program account must be made switchable during rollout.")
    } else if snippet.to_lowercase().contains("checked") {
        ("medium", "Checked operation requires mint decimal parity validation.")
    } else {
        ("low", "Straightforward CPI call site.")
    }
}

fn render_manifest(root: &Path, files_scanned: usize, findings: &[Finding], idl_hints: &[String]) -> String {
    let legacy_cu: u64 = findings.iter().map(|finding| finding.legacy_cu).sum();
    let p_token_cu: u64 = findings.iter().map(|finding| finding.p_token_cu).sum();
    let saved_cu = legacy_cu.saturating_sub(p_token_cu);
    let savings_percent = if legacy_cu == 0 {
        0.0
    } else {
        (saved_cu as f64 / legacy_cu as f64) * 100.0
    };
    let status = if findings.iter().any(|finding| finding.risk == "high") { "review_required" } else { "passed" };

    let mut json = String::new();
    json.push_str("{\n");
    json.push_str("  \"schemaVersion\": \"0.1.0\",\n");
    json.push_str(&format!("  \"root\": \"{}\",\n", escape_json(&root.display().to_string())));
    json.push_str("  \"pTokenProfile\": { \"name\": \"simd-0266-estimator\", \"status\": \"pre-mainnet-estimate\" },\n");
    json.push_str("  \"totals\": {\n");
    json.push_str(&format!("    \"filesScanned\": {},\n", files_scanned));
    json.push_str(&format!("    \"callSites\": {},\n", findings.len()));
    json.push_str(&format!("    \"legacyCu\": {},\n", legacy_cu));
    json.push_str(&format!("    \"pTokenCu\": {},\n", p_token_cu));
    json.push_str(&format!("    \"savedCu\": {},\n", saved_cu));
    json.push_str(&format!("    \"savingsPercent\": {:.1}\n", savings_percent));
    json.push_str("  },\n");
    json.push_str("  \"idlHints\": [");
    for (index, hint) in idl_hints.iter().enumerate() {
        if index > 0 {
            json.push_str(", ");
        }
        json.push_str(&format!("\"{}\"", escape_json(hint)));
    }
    json.push_str("],\n");
    json.push_str("  \"findings\": [\n");
    for (index, finding) in findings.iter().enumerate() {
        if index > 0 {
            json.push_str(",\n");
        }
        json.push_str("    {\n");
        json.push_str(&format!("      \"file\": \"{}\",\n", escape_json(&finding.file)));
        json.push_str(&format!("      \"line\": {},\n", finding.line));
        json.push_str(&format!("      \"operation\": \"{}\",\n", finding.operation));
        json.push_str(&format!("      \"snippet\": \"{}\",\n", escape_json(&finding.snippet)));
        json.push_str(&format!("      \"legacyCu\": {},\n", finding.legacy_cu));
        json.push_str(&format!("      \"pTokenCu\": {},\n", finding.p_token_cu));
        json.push_str(&format!("      \"savedCu\": {},\n", finding.legacy_cu.saturating_sub(finding.p_token_cu)));
        json.push_str(&format!("      \"risk\": \"{}\",\n", finding.risk));
        json.push_str(&format!("      \"riskReason\": \"{}\",\n", finding.risk_reason));
        json.push_str(&format!("      \"replacementPatch\": \"{}\"\n", escape_json(&replacement_patch(finding))));
        json.push_str("    }");
    }
    json.push_str("\n  ],\n");
    json.push_str(&format!("  \"simulation\": {{ \"status\": \"{}\", \"legacyRuns\": {}, \"pTokenRuns\": {} }}\n", status, findings.len(), findings.len()));
    json.push_str("}\n");
    json
}

fn replacement_patch(finding: &Finding) -> String {
    format!(
        "// {}:{}\nlet ctx = CpiContext::new(ctx.accounts.p_token_program.to_account_info(), PToken{} {{\n{}\n}});\np_token_shim::{}(ctx, amount)?;",
        finding.file,
        finding.line,
        pascal_case(finding.operation),
        replacement_accounts(finding.operation),
        finding.operation
    )
}

fn replacement_accounts(operation: &str) -> &'static str {
    match operation {
        "mint_to" => "    mint: ctx.accounts.mint.to_account_info(),\n    to: ctx.accounts.destination.to_account_info(),\n    authority: ctx.accounts.authority.to_account_info(),",
        "burn" => "    mint: ctx.accounts.mint.to_account_info(),\n    from: ctx.accounts.source.to_account_info(),\n    authority: ctx.accounts.authority.to_account_info(),",
        "close_account" => "    account: ctx.accounts.account.to_account_info(),\n    destination: ctx.accounts.destination.to_account_info(),\n    authority: ctx.accounts.authority.to_account_info(),",
        _ => "    source: ctx.accounts.source.to_account_info(),\n    destination: ctx.accounts.destination.to_account_info(),\n    authority: ctx.accounts.authority.to_account_info(),",
    }
}

fn pascal_case(value: &str) -> String {
    value
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

fn escape_json(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_signer_token_program_as_high_risk() {
        let (risk, _) = classify_risk("token_program with_signer seeds", "token::mint_to");
        assert_eq!(risk, "high");
    }

    #[test]
    fn pascal_cases_operations() {
        assert_eq!(pascal_case("close_account"), "CloseAccount");
    }
}
