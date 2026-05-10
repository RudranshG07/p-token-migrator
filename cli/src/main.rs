use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct Pattern {
    op: &'static str,
    anchor_names: &'static [&'static str],
    spl_names: &'static [&'static str],
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
    Pattern {
        op: "transfer",
        anchor_names: &["transfer", "transfer_checked"],
        spl_names: &["transfer", "transfer_checked"],
        legacy_cu: 5200,
        p_token_cu: 220,
    },
    Pattern {
        op: "mint_to",
        anchor_names: &["mint_to"],
        spl_names: &["mint_to"],
        legacy_cu: 6100,
        p_token_cu: 260,
    },
    Pattern {
        op: "burn",
        anchor_names: &["burn"],
        spl_names: &["burn"],
        legacy_cu: 5700,
        p_token_cu: 250,
    },
    Pattern {
        op: "approve",
        anchor_names: &["approve"],
        spl_names: &["approve"],
        legacy_cu: 4800,
        p_token_cu: 210,
    },
    Pattern {
        op: "close_account",
        anchor_names: &["close_account"],
        spl_names: &["close_account"],
        legacy_cu: 5000,
        p_token_cu: 240,
    },
    Pattern {
        op: "initialize_account",
        anchor_names: &["initialize_account"],
        spl_names: &["initialize_account"],
        legacy_cu: 7400,
        p_token_cu: 360,
    },
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
        let sanitized_source = sanitize_rust_source(&source);
        let source_lines: Vec<&str> = source.lines().collect();
        let sanitized_lines: Vec<&str> = sanitized_source.lines().collect();
        let aliases = detect_token_aliases(&sanitized_source);

        if file.relative.ends_with(".json")
            && (source.contains("\"tokenProgram\"")
                || source.contains("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"))
        {
            idl_hints.push(file.relative.clone());
        }

        for (line_index, line) in sanitized_lines.iter().enumerate() {
            for pattern in detect_operation_matches(line, &aliases) {
                let snippet = source_lines
                    .get(line_index)
                    .copied()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let (risk, risk_reason) = classify_risk(&sanitized_source, &snippet);
                findings.push(Finding {
                    file: file.relative.clone(),
                    line: line_index + 1,
                    operation: pattern.op,
                    snippet,
                    legacy_cu: pattern.legacy_cu,
                    p_token_cu: pattern.p_token_cu,
                    risk,
                    risk_reason,
                });
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
            if matches!(
                name.as_ref(),
                "node_modules" | ".git" | "dist" | ".next" | "target"
            ) {
                continue;
            }
            collect_files(root, &path, files)?;
        } else if matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("rs" | "json" | "toml")
        ) {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            files.push(ProjectFile {
                absolute: path,
                relative,
            });
        }
    }
    Ok(())
}

fn classify_risk(source: &str, snippet: &str) -> (&'static str, &'static str) {
    let lower = source.to_lowercase();
    let has_signer =
        lower.contains("signer") || lower.contains("with_signer") || lower.contains("seeds");
    let has_token_program = lower.contains("token_program");
    if has_signer && has_token_program {
        (
            "high",
            "Signer seeds and token program account constraints are present.",
        )
    } else if has_token_program {
        (
            "medium",
            "Token program account must be made switchable during rollout.",
        )
    } else if snippet.to_lowercase().contains("checked") {
        (
            "medium",
            "Checked operation requires mint decimal parity validation.",
        )
    } else {
        ("low", "Straightforward CPI call site.")
    }
}

fn sanitize_rust_source(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut output = String::with_capacity(source.len());
    let mut index = 0;
    let mut in_block_comment = false;
    let mut in_string = false;
    let mut escaped = false;

    while index < bytes.len() {
        let current = bytes[index] as char;
        let next = bytes.get(index + 1).map(|byte| *byte as char);

        if in_block_comment {
            if current == '*' && next == Some('/') {
                output.push(' ');
                output.push(' ');
                index += 2;
                in_block_comment = false;
            } else {
                output.push(if current == '\n' { '\n' } else { ' ' });
                index += 1;
            }
            continue;
        }

        if in_string {
            output.push(if current == '\n' { '\n' } else { ' ' });
            if escaped {
                escaped = false;
            } else if current == '\\' {
                escaped = true;
            } else if current == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }

        if current == '/' && next == Some('/') {
            while index < bytes.len() && bytes[index] as char != '\n' {
                output.push(' ');
                index += 1;
            }
            continue;
        }

        if current == '/' && next == Some('*') {
            output.push(' ');
            output.push(' ');
            index += 2;
            in_block_comment = true;
            continue;
        }

        if current == '"' {
            output.push(' ');
            index += 1;
            in_string = true;
            continue;
        }

        output.push(current);
        index += 1;
    }

    output
}

fn detect_token_aliases(source: &str) -> Vec<String> {
    let mut aliases = vec![
        "token".to_string(),
        "anchor_spl::token".to_string(),
        "spl_token::instruction".to_string(),
    ];

    for line in source.lines().map(str::trim) {
        if let Some(alias) = parse_use_alias(line, "use anchor_spl::token as ") {
            aliases.push(alias);
        }
        if let Some(alias) = parse_use_alias(line, "use spl_token::instruction as ") {
            aliases.push(alias);
        }
    }

    aliases.sort();
    aliases.dedup();
    aliases
}

fn parse_use_alias(line: &str, prefix: &str) -> Option<String> {
    let alias = line.strip_prefix(prefix)?.trim_end_matches(';').trim();
    if alias
        .chars()
        .all(|char| char == '_' || char.is_ascii_alphanumeric())
    {
        Some(alias.to_string())
    } else {
        None
    }
}

fn detect_operation_matches(line: &str, aliases: &[String]) -> Vec<&'static Pattern> {
    PATTERNS
        .iter()
        .filter(|pattern| {
            let anchor_match = pattern
                .anchor_names
                .iter()
                .any(|name| aliases.iter().any(|alias| contains_call(line, alias, name)));
            let spl_match = pattern.spl_names.iter().any(|name| {
                contains_call(line, "spl_token::instruction", name)
                    || aliases.iter().any(|alias| contains_call(line, alias, name))
            });
            anchor_match || spl_match
        })
        .collect()
}

fn contains_call(line: &str, namespace: &str, name: &str) -> bool {
    let needle = format!("{namespace}::{name}");
    let mut search_from = 0;

    while let Some(offset) = line[search_from..].find(&needle) {
        let start = search_from + offset;
        let end = start + needle.len();
        let previous = if start == 0 {
            None
        } else {
            line[..start].chars().next_back()
        };
        let previous_ok = previous.map(|char| !is_ident_char(char)).unwrap_or(true);
        let followed_by_call = line[end..].trim_start().starts_with('(');

        if previous_ok && followed_by_call {
            return true;
        }
        search_from = end;
    }

    false
}

fn is_ident_char(char: char) -> bool {
    char == '_' || char.is_ascii_alphanumeric()
}

fn render_manifest(
    root: &Path,
    files_scanned: usize,
    findings: &[Finding],
    idl_hints: &[String],
) -> String {
    let legacy_cu: u64 = findings.iter().map(|finding| finding.legacy_cu).sum();
    let p_token_cu: u64 = findings.iter().map(|finding| finding.p_token_cu).sum();
    let saved_cu = legacy_cu.saturating_sub(p_token_cu);
    let savings_percent = if legacy_cu == 0 {
        0.0
    } else {
        (saved_cu as f64 / legacy_cu as f64) * 100.0
    };
    let status = if findings.iter().any(|finding| finding.risk == "high") {
        "review_required"
    } else {
        "passed"
    };

    let mut json = String::new();
    json.push_str("{\n");
    json.push_str("  \"schemaVersion\": \"0.1.0\",\n");
    json.push_str(&format!(
        "  \"root\": \"{}\",\n",
        escape_json(&root.display().to_string())
    ));
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
        json.push_str(&format!(
            "      \"file\": \"{}\",\n",
            escape_json(&finding.file)
        ));
        json.push_str(&format!("      \"line\": {},\n", finding.line));
        json.push_str(&format!(
            "      \"operation\": \"{}\",\n",
            finding.operation
        ));
        json.push_str(&format!(
            "      \"snippet\": \"{}\",\n",
            escape_json(&finding.snippet)
        ));
        json.push_str(&format!("      \"legacyCu\": {},\n", finding.legacy_cu));
        json.push_str(&format!("      \"pTokenCu\": {},\n", finding.p_token_cu));
        json.push_str(&format!(
            "      \"savedCu\": {},\n",
            finding.legacy_cu.saturating_sub(finding.p_token_cu)
        ));
        json.push_str(&format!("      \"risk\": \"{}\",\n", finding.risk));
        json.push_str(&format!(
            "      \"riskReason\": \"{}\",\n",
            finding.risk_reason
        ));
        json.push_str(&format!(
            "      \"replacementPatch\": \"{}\"\n",
            escape_json(&replacement_patch(finding))
        ));
        json.push_str("    }");
    }
    json.push_str("\n  ],\n");
    json.push_str(&format!(
        "  \"simulation\": {{ \"status\": \"{}\", \"legacyRuns\": {}, \"pTokenRuns\": {} }}\n",
        status,
        findings.len(),
        findings.len()
    ));
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
        "mint_to" => {
            "    mint: ctx.accounts.mint.to_account_info(),\n    to: ctx.accounts.destination.to_account_info(),\n    authority: ctx.accounts.authority.to_account_info(),"
        }
        "burn" => {
            "    mint: ctx.accounts.mint.to_account_info(),\n    from: ctx.accounts.source.to_account_info(),\n    authority: ctx.accounts.authority.to_account_info(),"
        }
        "close_account" => {
            "    account: ctx.accounts.account.to_account_info(),\n    destination: ctx.accounts.destination.to_account_info(),\n    authority: ctx.accounts.authority.to_account_info(),"
        }
        _ => {
            "    source: ctx.accounts.source.to_account_info(),\n    destination: ctx.accounts.destination.to_account_info(),\n    authority: ctx.accounts.authority.to_account_info(),"
        }
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

    #[test]
    fn ignores_comments_strings_and_struct_literals() {
        let source = r#"
            // token::transfer(ctx, amount)?;
            let label = "spl_token::instruction::mint_to";
            let accounts = CloseAccount { account, destination, authority };
            token::burn(ctx, amount)?;
        "#;
        let sanitized = sanitize_rust_source(source);
        let aliases = detect_token_aliases(&sanitized);
        let operations: Vec<&str> = sanitized
            .lines()
            .flat_map(|line| detect_operation_matches(line, &aliases))
            .map(|pattern| pattern.op)
            .collect();

        assert_eq!(operations, vec!["burn"]);
    }

    #[test]
    fn detects_aliased_anchor_and_spl_calls() {
        let source = r#"
            use anchor_spl::token as token_cpi;
            use spl_token::instruction as token_ix;

            token_cpi::transfer(ctx, amount)?;
            token_ix::close_account(program_id, account, destination, authority, &[])?;
        "#;
        let sanitized = sanitize_rust_source(source);
        let aliases = detect_token_aliases(&sanitized);
        let operations: Vec<&str> = sanitized
            .lines()
            .flat_map(|line| detect_operation_matches(line, &aliases))
            .map(|pattern| pattern.op)
            .collect();

        assert_eq!(operations, vec!["transfer", "close_account"]);
    }
}
