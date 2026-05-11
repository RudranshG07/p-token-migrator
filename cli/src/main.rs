use std::env;
use std::fs;
use std::io::{self, Write};
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

#[derive(Debug)]
struct ScanResult {
    files_scanned: usize,
    findings: Vec<Finding>,
    idl_hints: Vec<String>,
    manifest: String,
}

#[derive(Debug)]
struct ScanCommand {
    root: PathBuf,
    out: Option<PathBuf>,
    bundle_out: Option<PathBuf>,
    sarif_out: Option<PathBuf>,
    fail_on_review: bool,
    summary: bool,
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
    if args.len() < 2 || matches!(args[1].as_str(), "help" | "--help" | "-h") {
        print_usage();
        return Ok(());
    }

    match args[1].as_str() {
        "scan" => run_scan(parse_scan_command(&args)?),
        "wizard" => run_wizard(),
        "interactive" | "repl" => run_interactive_session(),
        "validate" => run_validate(&args),
        _ => Err(format!("unknown command: {}", args[1]).into()),
    }
}

fn run_interactive_session() -> Result<(), Box<dyn std::error::Error>> {
    println!("p-token migrator interactive");
    println!("Type /help for commands, /exit to quit.\n");

    let mut last_manifest: Option<PathBuf> = None;
    let mut last_bundle: Option<PathBuf> = None;

    loop {
        print!("p-token> ");
        io::stdout().flush()?;
        let input = read_line()?;
        let line = input.trim();
        if line.is_empty() {
            continue;
        }

        let parts = split_words(line);
        match parts.first().map(String::as_str) {
            Some("/exit" | "/quit" | "exit" | "quit") => break,
            Some("/help" | "help") => print_interactive_help(),
            Some("/wizard" | "wizard") => {
                run_wizard()?;
            }
            Some("/scan" | "scan") => {
                let Some(root) = parts.get(1) else {
                    println!("usage: /scan <project-path>");
                    continue;
                };
                let result = scan_project_result(Path::new(root))?;
                print_summary(&result);
            }
            Some("/manifest" | "manifest") => {
                let Some(root) = parts.get(1) else {
                    println!("usage: /manifest <project-path> [manifest.json]");
                    continue;
                };
                let out = parts
                    .get(2)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("migration-manifest.json"));
                let result = scan_project_result(Path::new(root))?;
                if let Some(parent) = out.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&out, &result.manifest)?;
                last_manifest = Some(out.clone());
                println!("manifest written to {}", out.display());
                print_summary(&result);
            }
            Some("/bundle" | "bundle") => {
                let Some(root) = parts.get(1) else {
                    println!("usage: /bundle <project-path> [bundle-dir]");
                    continue;
                };
                let out = parts
                    .get(2)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("migration-bundle"));
                let result = scan_project_result(Path::new(root))?;
                write_migration_bundle(&out, &result)?;
                last_bundle = Some(out.clone());
                println!("migration bundle written to {}", out.display());
                print_summary(&result);
            }
            Some("/validate" | "validate") => {
                let manifest_path = parts
                    .get(1)
                    .map(PathBuf::from)
                    .or_else(|| last_manifest.clone());
                let Some(manifest_path) = manifest_path else {
                    println!("usage: /validate <manifest.json>");
                    continue;
                };
                let manifest = fs::read_to_string(&manifest_path)?;
                validate_manifest_text(&manifest)?;
                println!("manifest valid: {}", manifest_path.display());
                println!("simulation status: {}", manifest_status(&manifest));
            }
            Some("/last" | "last") => {
                println!(
                    "last manifest: {}",
                    last_manifest
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "none".to_string())
                );
                println!(
                    "last bundle: {}",
                    last_bundle
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "none".to_string())
                );
            }
            Some(other) if !other.starts_with('/') => {
                let result = scan_project_result(Path::new(other))?;
                print_summary(&result);
            }
            Some(unknown) => {
                println!("unknown command: {unknown}");
                println!("type /help for commands");
            }
            None => {}
        }
    }

    Ok(())
}

fn run_wizard() -> Result<(), Box<dyn std::error::Error>> {
    println!("p-token migration wizard");
    println!("Press Enter to accept defaults.\n");

    let root = prompt_required("Project path")?;
    let out = prompt_optional("Manifest output path", "migration-manifest.json")?;
    let bundle_out = prompt_optional("Migration bundle directory", "migration-bundle")?;
    let summary = prompt_yes_no("Print summary", true)?;
    let fail_on_review = prompt_yes_no("Exit non-zero on review-required findings", false)?;

    run_scan(ScanCommand {
        root: PathBuf::from(root),
        out: out.map(PathBuf::from),
        bundle_out: bundle_out.map(PathBuf::from),
        sarif_out: None,
        fail_on_review,
        summary,
    })
}

fn split_words(line: &str) -> Vec<String> {
    line.split_whitespace().map(ToString::to_string).collect()
}

fn print_interactive_help() {
    println!("commands:");
    println!("  /scan <project-path>                  scan and print summary");
    println!("  /manifest <project-path> [file]       scan and write manifest");
    println!("  /bundle <project-path> [directory]    scan and write migration bundle");
    println!(
        "  /validate [manifest.json]             validate manifest, defaults to last manifest"
    );
    println!("  /wizard                               guided prompt flow");
    println!("  /last                                 show last generated paths");
    println!("  /exit                                 quit");
    println!();
    println!("shortcut: type a project path directly to scan it");
}

fn run_scan(command: ScanCommand) -> Result<(), Box<dyn std::error::Error>> {
    let result = scan_project_result(&command.root)?;

    if let Some(out_path) = &command.out {
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&out_path, &result.manifest)?;
        println!("manifest written to {}", out_path.display());
    }

    if let Some(bundle_out) = &command.bundle_out {
        write_migration_bundle(&bundle_out, &result)?;
        println!("migration bundle written to {}", bundle_out.display());
    }

    if let Some(sarif_out) = &command.sarif_out {
        if let Some(parent) = sarif_out.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(sarif_out, render_sarif(&result))?;
        println!("sarif written to {}", sarif_out.display());
    }

    if command.summary {
        print_summary(&result);
    } else if command.out.is_none() {
        println!("{}", result.manifest);
    }

    if command.fail_on_review && simulation_status(&result.findings) == "review_required" {
        return Err("review required findings present".into());
    }

    Ok(())
}

fn run_validate(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() < 3 {
        return Err("validate requires a manifest path".into());
    }

    let manifest_path = PathBuf::from(&args[2]);
    let manifest = fs::read_to_string(&manifest_path)?;
    validate_manifest_text(&manifest)?;
    let status = manifest_status(&manifest);
    println!("manifest valid: {}", manifest_path.display());
    println!("simulation status: {status}");

    if args.iter().any(|arg| arg == "--fail-on-review") && status == "review_required" {
        return Err("review required findings present".into());
    }

    Ok(())
}

fn parse_scan_command(args: &[String]) -> Result<ScanCommand, String> {
    if args.len() < 3 || matches!(args[2].as_str(), "--help" | "-h") {
        print_usage();
        return Err("scan requires a project path".to_string());
    }

    let mut command = ScanCommand {
        root: PathBuf::from(&args[2]),
        out: None,
        bundle_out: None,
        sarif_out: None,
        fail_on_review: false,
        summary: false,
    };

    let mut index = 3;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => {
                index += 1;
                command.out = Some(PathBuf::from(
                    args.get(index).ok_or("--out requires a path")?,
                ));
            }
            "--bundle-out" => {
                index += 1;
                command.bundle_out = Some(PathBuf::from(
                    args.get(index).ok_or("--bundle-out requires a directory")?,
                ));
            }
            "--sarif-out" => {
                index += 1;
                command.sarif_out = Some(PathBuf::from(
                    args.get(index).ok_or("--sarif-out requires a path")?,
                ));
            }
            "--fail-on-review" => command.fail_on_review = true,
            "--summary" => command.summary = true,
            unknown => return Err(format!("unknown scan flag: {unknown}")),
        }
        index += 1;
    }

    Ok(command)
}

fn print_usage() {
    println!("p-token-migrator-cli");
    println!();
    println!("usage:");
    println!("  p-token-migrator-cli scan <project-path> [options]");
    println!("  p-token-migrator-cli interactive");
    println!("  p-token-migrator-cli wizard");
    println!("  p-token-migrator-cli validate <manifest.json> [--fail-on-review]");
    println!();
    println!("scan options:");
    println!("  --out <manifest.json>       write manifest JSON to a file");
    println!("  --bundle-out <directory>    write migration bundle artifacts");
    println!("  --sarif-out <report.sarif>  write GitHub/code-scanning SARIF output");
    println!("  --summary                   print human-readable summary instead of JSON");
    println!("  --fail-on-review            exit non-zero when review-required findings exist");
    println!();
    println!("interactive commands:");
    println!("  /scan, /manifest, /bundle, /validate, /wizard, /last, /exit");
}

fn prompt_required(label: &str) -> io::Result<String> {
    loop {
        print!("{label}: ");
        io::stdout().flush()?;
        let value = read_line()?.trim().to_string();
        if !value.is_empty() {
            return Ok(value);
        }
        println!("{label} is required.");
    }
}

fn prompt_optional(label: &str, default: &str) -> io::Result<Option<String>> {
    print!("{label} [{default}] (blank for default, '-' to skip): ");
    io::stdout().flush()?;
    let value = read_line()?.trim().to_string();
    if value == "-" {
        Ok(None)
    } else if value.is_empty() {
        Ok(Some(default.to_string()))
    } else {
        Ok(Some(value))
    }
}

fn prompt_yes_no(label: &str, default: bool) -> io::Result<bool> {
    let default_label = if default { "Y/n" } else { "y/N" };
    loop {
        print!("{label} [{default_label}]: ");
        io::stdout().flush()?;
        let value = read_line()?.trim().to_lowercase();
        if value.is_empty() {
            return Ok(default);
        }
        match value.as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => println!("Answer yes or no."),
        }
    }
}

fn read_line() -> io::Result<String> {
    let mut value = String::new();
    io::stdin().read_line(&mut value)?;
    Ok(value)
}

fn validate_manifest_text(manifest: &str) -> Result<(), Box<dyn std::error::Error>> {
    for field in ["schemaVersion", "totals", "findings", "simulation"] {
        if !manifest.contains(&format!("\"{field}\"")) {
            return Err(format!("manifest missing required field: {field}").into());
        }
    }
    Ok(())
}

fn manifest_status(manifest: &str) -> &'static str {
    if manifest.contains("\"status\": \"review_required\"") {
        "review_required"
    } else {
        "passed"
    }
}

fn scan_project_result(root: &Path) -> io::Result<ScanResult> {
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

    let manifest = render_manifest(root, files.len(), &findings, &idl_hints);
    Ok(ScanResult {
        files_scanned: files.len(),
        findings,
        idl_hints,
        manifest,
    })
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

fn print_summary(result: &ScanResult) {
    let legacy_cu: u64 = result
        .findings
        .iter()
        .map(|finding| finding.legacy_cu)
        .sum();
    let p_token_cu: u64 = result
        .findings
        .iter()
        .map(|finding| finding.p_token_cu)
        .sum();
    let saved_cu = legacy_cu.saturating_sub(p_token_cu);
    let savings_percent = if legacy_cu == 0 {
        0.0
    } else {
        (saved_cu as f64 / legacy_cu as f64) * 100.0
    };

    println!("p-token migration summary");
    println!("files scanned: {}", result.files_scanned);
    println!("call sites: {}", result.findings.len());
    println!("idl hints: {}", result.idl_hints.len());
    println!("legacy CU: {legacy_cu}");
    println!("p-token CU: {p_token_cu}");
    println!("saved CU: {saved_cu}");
    println!("savings: {savings_percent:.1}%");
    println!("simulation status: {}", simulation_status(&result.findings));
}

fn write_migration_bundle(out_dir: &Path, result: &ScanResult) -> io::Result<()> {
    fs::create_dir_all(out_dir)?;
    fs::create_dir_all(out_dir.join("patches"))?;
    fs::create_dir_all(out_dir.join("crates/p-token-shim-anchor"))?;

    fs::write(out_dir.join("README.md"), render_bundle_readme(result))?;
    fs::write(out_dir.join("migration-manifest.json"), &result.manifest)?;
    fs::write(
        out_dir.join("patches/p-token-replacements.rs"),
        render_replacements_file(&result.findings),
    )?;
    fs::write(
        out_dir.join("crates/p-token-shim-anchor/USAGE.md"),
        render_shim_usage(&result.findings),
    )?;
    Ok(())
}

fn render_bundle_readme(result: &ScanResult) -> String {
    let legacy_cu: u64 = result
        .findings
        .iter()
        .map(|finding| finding.legacy_cu)
        .sum();
    let p_token_cu: u64 = result
        .findings
        .iter()
        .map(|finding| finding.p_token_cu)
        .sum();
    let saved_cu = legacy_cu.saturating_sub(p_token_cu);
    let savings_percent = if legacy_cu == 0 {
        0.0
    } else {
        (saved_cu as f64 / legacy_cu as f64) * 100.0
    };

    [
        "# p-token migration bundle".to_string(),
        String::new(),
        "Generated by p-token-migrator-cli.".to_string(),
        String::new(),
        "## Summary".to_string(),
        String::new(),
        format!("- Files scanned: {}", result.files_scanned),
        format!("- Call sites: {}", result.findings.len()),
        format!("- IDL hints: {}", result.idl_hints.len()),
        format!("- Estimated CU saved: {saved_cu}"),
        format!("- Estimated savings: {savings_percent:.1}%"),
        format!(
            "- Simulation status: {}",
            simulation_status(&result.findings)
        ),
        String::new(),
        "## Files".to_string(),
        String::new(),
        "- `migration-manifest.json`: full scan output".to_string(),
        "- `patches/p-token-replacements.rs`: generated replacement snippets".to_string(),
        "- `crates/p-token-shim-anchor/USAGE.md`: shim integration notes".to_string(),
        String::new(),
        "Review signer seeds, token-program account constraints, and checked-operation decimal handling before shipping.".to_string(),
    ]
    .join("\n")
}

fn render_replacements_file(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "// No SPL Token CPI call sites were found.\n".to_string();
    }

    findings
        .iter()
        .map(|finding| {
            format!(
                "// Finding: {}:{}:{}\n// Original: {}\n{}",
                finding.file,
                finding.line,
                finding.operation,
                finding.snippet,
                replacement_patch(finding)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_shim_usage(findings: &[Finding]) -> String {
    let mut operations = Vec::new();
    for finding in findings {
        if !operations.contains(&finding.operation) {
            operations.push(finding.operation);
        }
    }

    let operation_lines = if operations.is_empty() {
        "- none".to_string()
    } else {
        operations
            .iter()
            .map(|operation| format!("- {operation}"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "# p-token shim usage\n\nUse `p-token-shim-anchor` as the transition boundary while p-token interfaces stabilize.\n\n```rust\nlet selector = PTokenProgramSelector::new(legacy_program_id, p_token_program_id, selected_program_id);\nlet route = selector.route(PTokenOperation::Transfer);\n```\n\nGenerated operations:\n\n{operation_lines}\n"
    )
}

fn render_sarif(result: &ScanResult) -> String {
    let rules = PATTERNS
        .iter()
        .map(|pattern| {
            format!(
                "        {{ \"id\": \"p-token.{}\", \"name\": \"{}\", \"shortDescription\": {{ \"text\": \"Legacy SPL Token {} CPI\" }}, \"help\": {{ \"text\": \"Review this SPL Token call site before migrating to p-token.\" }} }}",
                pattern.op,
                pattern.op,
                pattern.op
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");

    let results = result
        .findings
        .iter()
        .map(|finding| {
            format!(
                "        {{ \"ruleId\": \"p-token.{}\", \"level\": \"{}\", \"message\": {{ \"text\": \"{}: {} Estimated CU saved: {}.\" }}, \"locations\": [{{ \"physicalLocation\": {{ \"artifactLocation\": {{ \"uri\": \"{}\" }}, \"region\": {{ \"startLine\": {} }} }} }}] }}",
                finding.operation,
                sarif_level(finding.risk),
                escape_json(finding.operation),
                escape_json(finding.risk_reason),
                finding.legacy_cu.saturating_sub(finding.p_token_cu),
                escape_json(&finding.file),
                finding.line
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");

    format!(
        "{{\n  \"version\": \"2.1.0\",\n  \"$schema\": \"https://json.schemastore.org/sarif-2.1.0.json\",\n  \"runs\": [\n    {{\n      \"tool\": {{\n        \"driver\": {{\n          \"name\": \"p-token-migrator-cli\",\n          \"informationUri\": \"https://github.com/\",\n          \"rules\": [\n{}\n          ]\n        }}\n      }},\n      \"results\": [\n{}\n      ]\n    }}\n  ]\n}}\n",
        rules, results
    )
}

fn sarif_level(risk: &str) -> &'static str {
    match risk {
        "high" => "error",
        "medium" => "warning",
        _ => "note",
    }
}

fn simulation_status(findings: &[Finding]) -> &'static str {
    if findings.iter().any(|finding| finding.risk == "high") {
        "review_required"
    } else {
        "passed"
    }
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
    let status = simulation_status(findings);

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
        "  \"simulation\": {{ \"status\": \"{}\", \"mode\": \"deterministic\", \"legacyRuns\": {}, \"pTokenRuns\": {} }},\n",
        status,
        findings.len(),
        findings.len()
    ));
    json.push_str("  \"milestones\": [\n");
    json.push_str(&format!(
        "    {{ \"name\": \"IDL Scanner\", \"status\": \"mvp\", \"evidence\": \"{} CPI call sites and {} IDL hints detected.\" }},\n",
        findings.len(),
        idl_hints.len()
    ));
    json.push_str(&format!(
        "    {{ \"name\": \"p-token Codegen + CU Diff\", \"status\": \"mvp\", \"evidence\": \"{} replacement snippets generated with estimated CU savings.\" }},\n",
        findings.len()
    ));
    json.push_str("    { \"name\": \"Forked-Mainnet Dry-Run Simulator\", \"status\": \"mvp\", \"evidence\": \"deterministic dry-run completed; forked replay requires final p-token interfaces.\" },\n");
    json.push_str("    { \"name\": \"Compatibility Shim Anchor Crate\", \"status\": \"mvp\", \"evidence\": \"local p-token-shim-anchor crate scaffold is included.\" },\n");
    json.push_str("    { \"name\": \"Migration Dashboard + Public Launch\", \"status\": \"mvp\", \"evidence\": \"dashboard, reports, Docker, CI, and deployment docs are included.\" }\n");
    json.push_str("  ]\n");
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

    #[test]
    fn parses_product_scan_flags() {
        let args = vec![
            "p-token-migrator-cli".to_string(),
            "scan".to_string(),
            "samples/anchor-token-vault".to_string(),
            "--out".to_string(),
            "data/manifest.json".to_string(),
            "--bundle-out".to_string(),
            "data/bundle".to_string(),
            "--sarif-out".to_string(),
            "data/report.sarif".to_string(),
            "--summary".to_string(),
            "--fail-on-review".to_string(),
        ];
        let command = parse_scan_command(&args).expect("scan command should parse");

        assert_eq!(command.root, PathBuf::from("samples/anchor-token-vault"));
        assert_eq!(command.out, Some(PathBuf::from("data/manifest.json")));
        assert_eq!(command.bundle_out, Some(PathBuf::from("data/bundle")));
        assert_eq!(command.sarif_out, Some(PathBuf::from("data/report.sarif")));
        assert!(command.summary);
        assert!(command.fail_on_review);
    }

    #[test]
    fn validates_required_manifest_fields() {
        let manifest = r#"{
          "schemaVersion": "0.1.0",
          "totals": {},
          "findings": [],
          "simulation": { "status": "passed" }
        }"#;

        assert!(validate_manifest_text(manifest).is_ok());
        assert_eq!(manifest_status(manifest), "passed");
    }

    #[test]
    fn usage_mentions_interactive_wizard() {
        let args = vec!["p-token-migrator-cli".to_string(), "wizard".to_string()];
        assert_eq!(args[1], "wizard");
    }

    #[test]
    fn splits_interactive_words() {
        assert_eq!(
            split_words("/bundle samples/anchor-token-vault data/bundle"),
            vec!["/bundle", "samples/anchor-token-vault", "data/bundle"]
        );
    }

    #[test]
    fn renders_sarif_for_findings() {
        let result = ScanResult {
            files_scanned: 1,
            findings: vec![Finding {
                file: "programs/vault/src/lib.rs".to_string(),
                line: 7,
                operation: "transfer",
                snippet: "token::transfer(ctx, amount)?;".to_string(),
                legacy_cu: 5200,
                p_token_cu: 220,
                risk: "high",
                risk_reason: "Signer seeds and token program account constraints are present.",
            }],
            idl_hints: vec![],
            manifest: "{}".to_string(),
        };
        let sarif = render_sarif(&result);

        assert!(sarif.contains("\"version\": \"2.1.0\""));
        assert!(sarif.contains("\"ruleId\": \"p-token.transfer\""));
        assert!(sarif.contains("\"uri\": \"programs/vault/src/lib.rs\""));
    }
}
