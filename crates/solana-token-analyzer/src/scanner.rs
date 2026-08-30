use crate::manifest::{
    Compute, Confidence, Finding, IdlHint, Manifest, MilestoneStatus, MilestoneStatusLevel,
    Operation, PatternKind, ProfileSummary, SimulationReport, TokenProgram, Totals, SCHEMA_VERSION,
};
use crate::profile::{default_profile, BenchmarkProfile};
use crate::{risk, simulation};
use anyhow::{Context, Result};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, ExprCall, ExprPath, File as SynFile, Item, ItemFn, UseTree};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub relative: String,
    pub content: String,
}

#[derive(Debug, Clone, Default)]
pub struct ScanOptions {
    pub protocol: Option<String>,
    pub root: Option<String>,
    pub profile: Option<BenchmarkProfile>,
}

const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    ".git",
    "dist",
    ".next",
    "build",
    "web-dist",
];

pub fn scan_project(root: &Path, options: ScanOptions) -> Result<Manifest> {
    let absolute_root = fs::canonicalize(root)
        .with_context(|| format!("canonicalizing {}", root.display()))?;
    let files = list_project_files(&absolute_root)?;
    let mut sources = Vec::with_capacity(files.len());
    for path in files {
        let rel = path
            .strip_prefix(&absolute_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let content = fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        sources.push(SourceFile { relative: rel, content });
    }

    let mut opts = options;
    if opts.root.is_none() {
        opts.root = Some(absolute_root.to_string_lossy().to_string());
    }
    if opts.protocol.is_none() {
        opts.protocol = absolute_root
            .file_name()
            .map(|n| infer_protocol_name(&n.to_string_lossy()));
    }
    scan_source_files(sources, opts)
}

pub fn scan_source_files(files: Vec<SourceFile>, options: ScanOptions) -> Result<Manifest> {
    let protocol = options
        .protocol
        .unwrap_or_else(|| "Uploaded Project".to_string());
    let profile = options.profile.unwrap_or_else(default_profile);

    let mut findings: Vec<Finding> = Vec::new();
    let mut idl_hints: Vec<IdlHint> = Vec::new();
    let files_scanned = files.len();

    // --- Pass 1: parse + per-file analysis -----------------------------------
    let mut analyses: Vec<FileAnalysis> = Vec::new();
    for file in files.into_iter() {
        if file.relative.ends_with(".json") {
            if let Some(hint) = collect_idl_hint(&file) {
                idl_hints.push(hint);
            }
            continue;
        }
        if !file.relative.ends_with(".rs") {
            continue;
        }
        let parsed = match syn::parse_file(&file.content) {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };
        let aliases = build_alias_map(&parsed);
        let mut call_visitor = CallVisitor {
            aliases: &aliases,
            hits: Vec::new(),
        };
        call_visitor.visit_file(&parsed);
        let mut fn_visitor = FnDefVisitor {
            aliases: &aliases,
            defs: Vec::new(),
        };
        fn_visitor.visit_file(&parsed);

        analyses.push(FileAnalysis {
            file,
            parsed,
            direct_hits: call_visitor.hits,
            fn_defs: fn_visitor.defs,
        });
    }

    // --- Pass 2: cross-file wrapper closure ----------------------------------
    let wrappers = build_wrapper_map(&analyses);

    // --- Pass 3: emit direct + wrapper findings -----------------------------
    for analysis in &analyses {
        let line_index: Vec<&str> = analysis.file.content.lines().collect();
        let mut emitted: HashSet<(usize, Operation)> = HashSet::new();

        // 3a — direct CPI findings (existing behavior)
        for hit in &analysis.direct_hits {
            if !emitted.insert((hit.line, hit.op)) {
                continue;
            }
            findings.push(make_finding(
                &analysis.file,
                &line_index,
                hit.line,
                hit.op,
                hit.token_program,
                hit.pattern_kind,
                None,
                &profile,
            ));
        }

        // 3b — wrapper-call findings (cross-file resolution)
        let mut wrapper_visitor = WrapperCallVisitor {
            wrappers: &wrappers,
            direct_lines: emitted.clone(),
            hits: Vec::new(),
        };
        wrapper_visitor.visit_file(&analysis.parsed);

        // Dedup wrapper hits by (line, op, callee) so multiple ops on one
        // call site emit at most one finding per op.
        let mut wrapper_emitted: HashSet<(usize, Operation, String)> = HashSet::new();
        for hit in wrapper_visitor.hits {
            for (op, program) in &hit.ops {
                if !wrapper_emitted.insert((hit.line, *op, hit.callee.clone())) {
                    continue;
                }
                // If this exact (line, op) was already emitted as a direct
                // hit, skip — avoids double-counting `token::transfer(..)`
                // when "transfer" also happens to be a wrapper name.
                if emitted.contains(&(hit.line, *op)) {
                    continue;
                }
                findings.push(make_finding(
                    &analysis.file,
                    &line_index,
                    hit.line,
                    *op,
                    *program,
                    PatternKind::WrappedCpi,
                    Some(hit.callee.clone()),
                    &profile,
                ));
            }
        }
    }

    let totals_legacy: u64 = findings.iter().map(|f| f.compute.legacy_cu).sum();
    let totals_ptoken: u64 = findings.iter().map(|f| f.compute.p_token_cu).sum();
    let totals_saved = totals_legacy.saturating_sub(totals_ptoken);
    let totals_pct = round_one_decimal(if totals_legacy > 0 {
        (totals_saved as f64 / totals_legacy as f64) * 100.0
    } else {
        0.0
    });

    let simulation = simulation::run_dry_run(&findings, &idl_hints);
    let milestones = build_milestones(&findings, &idl_hints, &simulation);

    Ok(Manifest {
        schema_version: SCHEMA_VERSION.to_string(),
        generated_at: current_timestamp(),
        protocol,
        root: options.root.unwrap_or_default(),
        p_token_profile: ProfileSummary {
            name: profile.name,
            status: profile.status,
            note: profile.note,
        },
        totals: Totals {
            legacy_cu: totals_legacy,
            p_token_cu: totals_ptoken,
            saved_cu: totals_saved,
            savings_percent: totals_pct,
            call_sites: findings.len(),
            files_scanned,
        },
        idl_hints,
        findings,
        simulation,
        milestones,
    })
}

#[derive(Debug, Clone, Copy)]
struct Hit {
    line: usize,
    op: Operation,
    pattern_kind: PatternKind,
    token_program: TokenProgram,
}

struct FileAnalysis {
    file: SourceFile,
    parsed: SynFile,
    direct_hits: Vec<Hit>,
    fn_defs: Vec<FunctionDef>,
}

#[derive(Debug, Clone)]
struct FunctionDef {
    name: String,
    /// Ops this function directly performs in its body.
    direct: BTreeSet<(Operation, TokenProgram)>,
    /// Simple names of functions this body calls (where the call isn't
    /// itself a token-program CPI).
    callees: Vec<String>,
}

struct CallVisitor<'a> {
    aliases: &'a AliasMap,
    hits: Vec<Hit>,
}

impl<'a, 'ast> Visit<'ast> for CallVisitor<'a> {
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if let Expr::Path(ExprPath { path, .. }) = node.func.as_ref() {
            if let Some((kind, program, op)) = match_call_path(path, self.aliases) {
                let line = path.span().start().line.max(1);
                self.hits.push(Hit {
                    line,
                    op,
                    pattern_kind: kind,
                    token_program: program,
                });
            }
        }
        syn::visit::visit_expr_call(self, node);
    }
}

// ---------------------------------------------------------------------------
// Function definitions + wrapper closure
// ---------------------------------------------------------------------------

struct FnDefVisitor<'a> {
    aliases: &'a AliasMap,
    defs: Vec<FunctionDef>,
}

impl<'a, 'ast> Visit<'ast> for FnDefVisitor<'a> {
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        self.collect(node.sig.ident.to_string(), &node.block);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        // Methods inside `impl Foo {}` blocks. Bare name, no receiver
        // disambiguation — same simple-name-match limitation as free fns.
        self.collect(node.sig.ident.to_string(), &node.block);
    }
}

impl<'a> FnDefVisitor<'a> {
    fn collect(&mut self, name: String, block: &syn::Block) {
        let mut body = FnBodyVisitor {
            aliases: self.aliases,
            direct: BTreeSet::new(),
            callees: Vec::new(),
        };
        body.visit_block(block);
        self.defs.push(FunctionDef {
            name,
            direct: body.direct,
            callees: body.callees,
        });
    }
}

struct FnBodyVisitor<'a> {
    aliases: &'a AliasMap,
    direct: BTreeSet<(Operation, TokenProgram)>,
    callees: Vec<String>,
}

impl<'a, 'ast> Visit<'ast> for FnBodyVisitor<'a> {
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if let Expr::Path(ExprPath { path, .. }) = node.func.as_ref() {
            if let Some((_, program, op)) = match_call_path(path, self.aliases) {
                self.direct.insert((op, program));
            } else if let Some(last) = path.segments.last() {
                self.callees.push(last.ident.to_string());
            }
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        // `receiver.method(args)` — record the method name as a potential
        // wrapper. Token-program CPIs are never methods (they're
        // free functions), so we never produce a direct hit here.
        self.callees.push(node.method.to_string());
        syn::visit::visit_expr_method_call(self, node);
    }
}

type WrapperMap = HashMap<String, BTreeSet<(Operation, TokenProgram)>>;

/// Build the transitive map: function name → set of (op, token_program) it
/// performs either directly or by calling another wrapper. Names that map to
/// an empty set are pruned, so the result contains only functions that
/// actually transitively touch a token CPI.
///
/// **Name-collision filter:** If the same function name is defined more than
/// once in the project and not every definition transitively touches a CPI
/// (e.g., a generic `setup` exists in both a fuzz harness and a wrapper
/// helper), the wrapper map entry is dropped. False positives at call sites
/// outweigh the value of speculatively emitting findings.
fn build_wrapper_map(analyses: &[FileAnalysis]) -> WrapperMap {
    // Pass 1 — seed with direct ops per function definition. Track the
    // number of definitions per name so we can resolve collisions later.
    let mut map: WrapperMap = HashMap::new();
    let mut total_defs: HashMap<String, usize> = HashMap::new();
    for analysis in analyses {
        for fd in &analysis.fn_defs {
            *total_defs.entry(fd.name.clone()).or_default() += 1;
            let entry = map.entry(fd.name.clone()).or_default();
            for pair in &fd.direct {
                entry.insert(*pair);
            }
        }
    }

    // Pass 2 — fixpoint over outgoing callees. Bounded by the number of
    // (op, program) pairs in the universe times the number of defs.
    loop {
        let mut changed = false;
        for analysis in analyses {
            for fd in &analysis.fn_defs {
                let mut to_add: BTreeSet<(Operation, TokenProgram)> = BTreeSet::new();
                for callee in &fd.callees {
                    if let Some(pairs) = map.get(callee) {
                        for pair in pairs {
                            to_add.insert(*pair);
                        }
                    }
                }
                if to_add.is_empty() {
                    continue;
                }
                let entry = map.entry(fd.name.clone()).or_default();
                let before = entry.len();
                for pair in to_add {
                    entry.insert(pair);
                }
                if entry.len() != before {
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    // Pass 3 — count how many definitions of each name transitively touch
    // a CPI. A name is a "wrapper" only if it has at least one such
    // definition.
    let mut cpi_defs: HashMap<String, usize> = HashMap::new();
    for analysis in analyses {
        for fd in &analysis.fn_defs {
            // A def is a CPI-touching def if it has direct ops OR if any of
            // its callees resolves to a non-empty map entry.
            let direct = !fd.direct.is_empty();
            let transitive = fd
                .callees
                .iter()
                .any(|c| map.get(c).map(|s| !s.is_empty()).unwrap_or(false));
            if direct || transitive {
                *cpi_defs.entry(fd.name.clone()).or_default() += 1;
            }
        }
    }

    // Pass 4 — prune. Two reasons a name leaves the map:
    //   (a) empty op set: nothing transitive at all.
    //   (b) name collision with a non-CPI definition: ambiguous, so dropping
    //       prevents false positives at call sites like `setup(...)`.
    map.retain(|name, set| {
        if set.is_empty() {
            return false;
        }
        let total = total_defs.get(name).copied().unwrap_or(0);
        let cpi = cpi_defs.get(name).copied().unwrap_or(0);
        // Only keep if EVERY definition of this name transitively touches
        // a CPI. If there's at least one collision with an unrelated
        // function, we can't tell which one a call site refers to.
        total == cpi
    });
    map
}

struct WrapperHit {
    line: usize,
    callee: String,
    ops: BTreeSet<(Operation, TokenProgram)>,
}

struct WrapperCallVisitor<'a> {
    wrappers: &'a WrapperMap,
    /// (line, op) pairs that were already emitted as direct hits — used so
    /// `token::transfer(..)` doesn't also produce a wrapper finding when
    /// some unrelated function happens to be named `transfer`.
    direct_lines: HashSet<(usize, Operation)>,
    hits: Vec<WrapperHit>,
}

impl<'a, 'ast> Visit<'ast> for WrapperCallVisitor<'a> {
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if let Expr::Path(ExprPath { path, .. }) = node.func.as_ref() {
            if let Some(last) = path.segments.last() {
                let name = last.ident.to_string();
                self.record(name, path.span().start().line.max(1));
            }
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        // `receiver.method(args)` — match the method name against the
        // wrapper map. The receiver's type isn't resolved (would require
        // type-level analysis); accepted as a known limitation.
        let name = node.method.to_string();
        let line = node.method.span().start().line.max(1);
        self.record(name, line);
        syn::visit::visit_expr_method_call(self, node);
    }
}

impl<'a> WrapperCallVisitor<'a> {
    fn record(&mut self, name: String, line: usize) {
        if let Some(ops) = self.wrappers.get(&name) {
            let already_direct = ops
                .iter()
                .all(|(op, _)| self.direct_lines.contains(&(line, *op)));
            if !already_direct {
                self.hits.push(WrapperHit {
                    line,
                    callee: name,
                    ops: ops.clone(),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Finding construction
// ---------------------------------------------------------------------------

fn make_finding(
    file: &SourceFile,
    line_index: &[&str],
    line: usize,
    op: Operation,
    token_program: TokenProgram,
    pattern_kind: PatternKind,
    via_wrapper: Option<String>,
    profile: &BenchmarkProfile,
) -> Finding {
    let snippet = line_index
        .get(line.saturating_sub(1))
        .copied()
        .unwrap_or("")
        .trim()
        .to_string();
    let cu = profile.lookup(op);
    let saved = cu.legacy_cu.saturating_sub(cu.p_token_cu);
    let savings_percent = round_one_decimal(if cu.legacy_cu > 0 {
        (saved as f64 / cu.legacy_cu as f64) * 100.0
    } else {
        0.0
    });
    let risk_value = risk::classify(&file.content, &snippet, op, token_program);
    let id_suffix = via_wrapper
        .as_ref()
        .map(|w| format!(":via:{w}"))
        .unwrap_or_default();
    let id = sanitize_id(&format!(
        "{}:{}:{}{}",
        file.relative,
        line,
        op.as_str(),
        id_suffix
    ));
    let replacement_patch = build_replacement_patch(op, &file.relative, line, token_program);
    Finding {
        id,
        file: file.relative.clone(),
        line,
        operation: op,
        snippet,
        confidence: if via_wrapper.is_some() {
            Confidence::Medium
        } else {
            Confidence::High
        },
        compute: Compute {
            legacy_cu: cu.legacy_cu,
            p_token_cu: cu.p_token_cu,
            saved_cu: saved,
            savings_percent,
        },
        risk: risk_value,
        replacement_patch,
        token_program,
        pattern_kind,
        via_wrapper,
    }
}

type AliasMap = HashMap<String, Vec<String>>;

fn build_alias_map(file: &SynFile) -> AliasMap {
    let mut map: AliasMap = HashMap::new();
    for (alias, canonical) in DEFAULT_ALIASES {
        map.insert(
            (*alias).to_string(),
            canonical.iter().map(|s| (*s).to_string()).collect(),
        );
    }
    for item in &file.items {
        if let Item::Use(item_use) = item {
            walk_use_tree(&item_use.tree, &[], &mut map);
        }
    }
    map
}

const DEFAULT_ALIASES: &[(&str, &[&str])] = &[
    ("anchor_spl", &["anchor_spl"]),
    ("spl_token", &["spl_token"]),
    ("spl_token_2022", &["spl_token_2022"]),
    ("anchor_lang", &["anchor_lang"]),
];

fn walk_use_tree(tree: &UseTree, prefix: &[String], map: &mut AliasMap) {
    match tree {
        UseTree::Path(p) => {
            let mut next = prefix.to_vec();
            next.push(p.ident.to_string());
            walk_use_tree(&p.tree, &next, map);
        }
        UseTree::Name(n) => {
            let mut full = prefix.to_vec();
            full.push(n.ident.to_string());
            map.insert(n.ident.to_string(), full);
        }
        UseTree::Rename(r) => {
            let mut full = prefix.to_vec();
            full.push(r.ident.to_string());
            map.insert(r.rename.to_string(), full);
        }
        UseTree::Glob(_) => {
            if !prefix.is_empty() {
                let sentinel = format!("__glob__{}", prefix.join("::"));
                map.entry(sentinel).or_insert_with(|| prefix.to_vec());
            }
        }
        UseTree::Group(g) => {
            for child in &g.items {
                if let UseTree::Name(n) = child {
                    if n.ident == "self" {
                        if let Some(last) = prefix.last() {
                            map.insert(last.clone(), prefix.to_vec());
                            continue;
                        }
                    }
                }
                walk_use_tree(child, prefix, map);
            }
        }
    }
}

fn match_call_path(
    path: &syn::Path,
    aliases: &AliasMap,
) -> Option<(PatternKind, TokenProgram, Operation)> {
    let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    if segments.is_empty() {
        return None;
    }
    let expanded = expand_with_aliases(&segments, aliases);
    if expanded.len() < 2 {
        return None;
    }
    let leaf = expanded.last()?.as_str();
    let op = Operation::from_name(leaf)?;

    // Match on the *last two segments* of the prefix (one segment before
    // the leaf). This is robust against re-export aliasing — e.g.,
    // marginfi-v2 has `use anchor_spl::token_2022::spl_token_2022;` which
    // expands `spl_token_2022::onchain::invoke_transfer_checked(...)` into
    // `anchor_spl::token_2022::spl_token_2022::onchain::invoke_transfer_checked`.
    // Last-two-segments matching catches `spl_token_2022::onchain` regardless
    // of how many crate boundaries precede it.
    let prefix = &expanded[..expanded.len() - 1];
    let last_two = if prefix.len() >= 2 {
        format!("{}::{}", prefix[prefix.len() - 2], prefix[prefix.len() - 1])
    } else {
        return None;
    };

    let (kind, program) = match last_two.as_str() {
        "anchor_spl::token" => (PatternKind::AnchorCpi, TokenProgram::SplToken),
        "anchor_spl::token_2022" => (PatternKind::AnchorCpi, TokenProgram::SplToken2022),
        "anchor_spl::token_interface" => {
            (PatternKind::TokenInterface, TokenProgram::TokenInterface)
        }
        "spl_token::instruction" => (PatternKind::SplInstruction, TokenProgram::SplToken),
        "spl_token_2022::instruction" => {
            (PatternKind::SplInstruction, TokenProgram::SplToken2022)
        }
        // `spl_token_2022::onchain::invoke_transfer_checked` — helper that
        // builds + invokes a Transfer with transfer-hook resolution.
        // Production usage: marginfi-v2 state/bank.rs:660,707.
        "spl_token_2022::onchain" => {
            (PatternKind::SplInstruction, TokenProgram::SplToken2022)
        }
        _ => return None,
    };
    Some((kind, program, op))
}

fn expand_with_aliases(segments: &[String], aliases: &AliasMap) -> Vec<String> {
    if segments.is_empty() {
        return segments.to_vec();
    }
    if let Some(canonical) = aliases.get(&segments[0]) {
        let mut out = canonical.clone();
        out.extend_from_slice(&segments[1..]);
        return out;
    }
    segments.to_vec()
}

fn collect_idl_hint(file: &SourceFile) -> Option<IdlHint> {
    let s = &file.content;
    let has_name = contains_name_field(s, "tokenProgram");
    let has_legacy_address = s.contains("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
    if !(has_name || has_legacy_address) {
        return None;
    }
    Some(IdlHint {
        file: file.relative.clone(),
        kind: "legacy_token_program_reference".to_string(),
        message: "IDL contains a legacy SPL Token program account reference.".to_string(),
    })
}

fn contains_name_field(haystack: &str, value: &str) -> bool {
    // Tolerate any whitespace between "name", ":", and the quoted value.
    let needle = format!("\"{}\"", value);
    if !haystack.contains(&needle) {
        return false;
    }
    for (idx, _) in haystack.match_indices(&needle) {
        let before = &haystack[..idx];
        let trimmed = before.trim_end_matches([' ', '\t', '\n', '\r']);
        if !trimmed.ends_with(':') {
            continue;
        }
        let pre_colon = trimmed.trim_end_matches(':').trim_end();
        if pre_colon.ends_with("\"name\"") {
            return true;
        }
    }
    false
}

fn list_project_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let walker = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 {
                return true;
            }
            if entry.file_type().is_dir() {
                let name = entry.file_name().to_string_lossy();
                !SKIP_DIRS.iter().any(|d| *d == name.as_ref())
            } else {
                true
            }
        });
    for entry in walker {
        let entry = entry.context("walking project")?;
        if !entry.file_type().is_file() {
            continue;
        }
        let ext = entry
            .path()
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if matches!(ext, "rs" | "json" | "toml") {
            out.push(entry.into_path());
        }
    }
    Ok(out)
}

fn build_replacement_patch(
    op: Operation,
    file: &str,
    line: usize,
    program: TokenProgram,
) -> String {
    if matches!(program, TokenProgram::TokenInterface) {
        return format!(
            "// {file}:{line}\n// token_interface call already dispatches at runtime; no replacement required.\n// Verify the *_2022 program ID is wired and any token-2022 extensions are handled."
        );
    }
    let account_hint = match op {
        Operation::Transfer => "PTokenTransfer".to_string(),
        other => format!("PToken{}", other.pascal()),
    };
    let accounts = replacement_accounts(op);
    let mut out = String::new();
    out.push_str(&format!("// {file}:{line}\n"));
    out.push_str(&format!(
        "let ctx = CpiContext::new(ctx.accounts.p_token_program.to_account_info(), {account_hint} {{\n"
    ));
    for a in accounts {
        out.push_str(a);
        out.push('\n');
    }
    out.push_str("});\n");
    out.push_str(&format!("p_token_shim::{}(ctx, amount)?;", op.as_str()));
    out
}

fn replacement_accounts(op: Operation) -> &'static [&'static str] {
    match op {
        Operation::MintTo => &[
            "    mint: ctx.accounts.mint.to_account_info(),",
            "    to: ctx.accounts.destination.to_account_info(),",
            "    authority: ctx.accounts.authority.to_account_info(),",
        ],
        Operation::Burn => &[
            "    mint: ctx.accounts.mint.to_account_info(),",
            "    from: ctx.accounts.source.to_account_info(),",
            "    authority: ctx.accounts.authority.to_account_info(),",
        ],
        Operation::CloseAccount => &[
            "    account: ctx.accounts.account.to_account_info(),",
            "    destination: ctx.accounts.destination.to_account_info(),",
            "    authority: ctx.accounts.authority.to_account_info(),",
        ],
        _ => &[
            "    source: ctx.accounts.source.to_account_info(),",
            "    destination: ctx.accounts.destination.to_account_info(),",
            "    authority: ctx.accounts.authority.to_account_info(),",
        ],
    }
}

fn build_milestones(
    findings: &[Finding],
    idl_hints: &[IdlHint],
    sim: &SimulationReport,
) -> Vec<MilestoneStatus> {
    vec![
        MilestoneStatus {
            name: "AST Scanner".to_string(),
            status: MilestoneStatusLevel::Complete,
            evidence: format!(
                "{} CPI call sites and {} IDL hints detected via syn AST parsing.",
                findings.len(),
                idl_hints.len()
            ),
        },
        MilestoneStatus {
            name: "p-token Codegen + CU Diff".to_string(),
            status: MilestoneStatusLevel::Mvp,
            evidence: format!(
                "{} replacement snippets generated with estimated CU savings.",
                findings.len()
            ),
        },
        MilestoneStatus {
            name: "CU Measurement Harness".to_string(),
            status: MilestoneStatusLevel::Blocked,
            evidence: "LiteSVM-based measurement pipeline pending (step 2 of platform roadmap)."
                .to_string(),
        },
        MilestoneStatus {
            name: "Forked-Mainnet Replay".to_string(),
            status: MilestoneStatusLevel::Blocked,
            evidence: format!(
                "{} dry-run produced {} review items via deterministic heuristic.",
                sim.mode,
                sim.divergences.len()
            ),
        },
        MilestoneStatus {
            name: "Token-2022 Analysis".to_string(),
            status: MilestoneStatusLevel::Mvp,
            evidence: "Token-2022 and token_interface call sites are tagged in findings.".to_string(),
        },
    ]
}

fn sanitize_id(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '.' | '/' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn round_one_decimal(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn infer_protocol_name(name: &str) -> String {
    let cleaned = name.replace(['-', '_'], " ");
    cleaned
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_iso8601(secs)
}

fn format_iso8601(secs: u64) -> String {
    const SECONDS_PER_DAY: u64 = 86_400;
    let days = (secs / SECONDS_PER_DAY) as i64;
    let time_of_day = secs % SECONDS_PER_DAY;
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;
    let (y, m, d) = civil_from_days(days);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, hour, minute, second)
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z / 146_097 } else { (z - 146_096) / 146_097 };
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 {
        y += 1;
    }
    (y as i32, m as u32, d as u32)
}
