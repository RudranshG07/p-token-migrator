import { promises as fs } from "node:fs";
import path from "node:path";

type Operation =
  | "transfer"
  | "mint_to"
  | "burn"
  | "approve"
  | "close_account"
  | "initialize_account";

type Confidence = "high" | "medium";
type RiskLevel = "low" | "medium" | "high";
type SimulationStatus = "passed" | "review_required";

interface TokenPattern {
  op: Operation;
  regex: RegExp;
}

export interface BenchmarkProfile {
  name: string;
  status: string;
  note: string;
  operations: Record<Operation, {
    legacyCu: number;
    pTokenCu: number;
  }>;
}

export interface ProjectFile {
  absolute: string;
  relative: string;
}

export interface SourceFile {
  relative: string;
  content: string;
}

export interface IdlHint {
  file: string;
  type: "legacy_token_program_reference";
  message: string;
}

export interface Finding {
  id: string;
  file: string;
  line: number;
  operation: Operation;
  snippet: string;
  confidence: Confidence;
  compute: {
    legacyCu: number;
    pTokenCu: number;
    savedCu: number;
    savingsPercent: number;
  };
  risk: {
    level: RiskLevel;
    reason: string;
  };
  replacementPatch: string;
}

export interface Manifest {
  schemaVersion: string;
  generatedAt: string;
  protocol: string;
  root: string;
  pTokenProfile: {
    name: string;
    status: string;
    note: string;
  };
  totals: {
    legacyCu: number;
    pTokenCu: number;
    savedCu: number;
    savingsPercent: number;
    callSites: number;
    filesScanned: number;
  };
  idlHints: IdlHint[];
  findings: Finding[];
  simulation: SimulationReport;
}

export interface SimulationReport {
  status: SimulationStatus;
  legacyRuns: number;
  pTokenRuns: number;
  divergences: Array<{
    findingId: string;
    severity: "review";
    reason: string;
  }>;
}

export interface ScanOptions {
  protocol?: string;
  benchmarkProfile?: BenchmarkProfile;
}

export interface Job {
  id: string;
  protocol: string;
  createdAt: string;
  totals: Manifest["totals"];
  simulation: SimulationReport;
  report: ReportSummary;
  manifest?: Manifest;
}

export interface ReportSummary {
  id?: string;
  protocol: string;
  generatedAt: string;
  totals: Manifest["totals"];
  simulation: SimulationReport;
  operations: Record<string, number>;
  risks: Record<RiskLevel, number>;
  files: Array<{
    file: string;
    findings: number;
  }>;
}

export interface SaveJobOptions {
  storeManifest?: boolean;
  retentionLimit?: number;
}

interface BuildFindingInput {
  file: string;
  line: number;
  op: Operation;
  snippet: string;
  legacyCu: number;
  pTokenCu: number;
  source: string;
}

const TOKEN_PATTERNS: TokenPattern[] = [
  { op: "transfer", regex: /\b(token::transfer|transfer_checked|TransferChecked|Transfer\s*\{|spl_token::instruction::transfer)\b/g },
  { op: "mint_to", regex: /\b(token::mint_to|MintTo\s*\{|spl_token::instruction::mint_to)\b/g },
  { op: "burn", regex: /\b(token::burn|Burn\s*\{|spl_token::instruction::burn)\b/g },
  { op: "approve", regex: /\b(token::approve|Approve\s*\{|spl_token::instruction::approve)\b/g },
  { op: "close_account", regex: /\b(token::close_account|CloseAccount\s*\{|spl_token::instruction::close_account)\b/g },
  { op: "initialize_account", regex: /\b(InitializeAccount|initialize_account|spl_token::instruction::initialize_account)\b/g }
];

export const DEFAULT_BENCHMARK_PROFILE: BenchmarkProfile = {
  name: "simd-0266-estimator",
  status: "pre-mainnet-estimate",
  note: "CU estimates are based on the bundled SIMD-0266 estimator profile until p-token interfaces are finalized.",
  operations: {
    transfer: { legacyCu: 5200, pTokenCu: 220 },
    mint_to: { legacyCu: 6100, pTokenCu: 260 },
    burn: { legacyCu: 5700, pTokenCu: 250 },
    approve: { legacyCu: 4800, pTokenCu: 210 },
    close_account: { legacyCu: 5000, pTokenCu: 240 },
    initialize_account: { legacyCu: 7400, pTokenCu: 360 }
  }
};

const IDL_ACCOUNT_PATTERN = /"name"\s*:\s*"tokenProgram"|"address"\s*:\s*"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"/g;
const SUPPORTED_EXTENSIONS = new Set([".rs", ".json", ".toml"]);

export function getSampleProjectPath(): string {
  return path.resolve("samples/anchor-token-vault");
}

export async function listProjectFiles(rootDir: string): Promise<ProjectFile[]> {
  const root = path.resolve(rootDir);
  const files: ProjectFile[] = [];

  async function walk(current: string): Promise<void> {
    const entries = await fs.readdir(current, { withFileTypes: true });
    for (const entry of entries) {
      const absolute = path.join(current, entry.name);
      const relative = path.relative(root, absolute);
      if (entry.isDirectory()) {
        if (["node_modules", "target", ".git", "dist", ".next"].includes(entry.name)) continue;
        await walk(absolute);
      } else if (SUPPORTED_EXTENSIONS.has(path.extname(entry.name))) {
        files.push({ absolute, relative });
      }
    }
  }

  await walk(root);
  return files;
}

export async function scanProject(projectPath: string, options: ScanOptions = {}): Promise<Manifest> {
  const root = path.resolve(projectPath);
  const protocol = options.protocol || inferProtocolName(root);
  const files = await listProjectFiles(root);
  const sourceFiles = await Promise.all(files.map(async (file) => ({
    relative: file.relative,
    content: await fs.readFile(file.absolute, "utf8")
  })));
  return scanSourceFiles(sourceFiles, { protocol, root, benchmarkProfile: options.benchmarkProfile });
}

export async function scanSourceFiles(files: SourceFile[], options: ScanOptions & { root?: string } = {}): Promise<Manifest> {
  const protocol = options.protocol || "Uploaded Project";
  const benchmarkProfile = options.benchmarkProfile || DEFAULT_BENCHMARK_PROFILE;
  const findings: Finding[] = [];
  const idlHints: IdlHint[] = [];

  for (const file of files) {
    const source = file.content;
    const lines = source.split(/\r?\n/);

    if (file.relative.endsWith(".json")) {
      collectIdlHints(source, file.relative, idlHints);
    }

    for (let index = 0; index < lines.length; index += 1) {
      const line = lines[index] || "";
      for (const pattern of TOKEN_PATTERNS) {
        pattern.regex.lastIndex = 0;
        if (!pattern.regex.test(line)) continue;
        const compute = benchmarkProfile.operations[pattern.op];
        findings.push(buildFinding({
          file: file.relative,
          line: index + 1,
          op: pattern.op,
          snippet: line.trim(),
          legacyCu: compute.legacyCu,
          pTokenCu: compute.pTokenCu,
          source
        }));
      }
    }
  }

  const totals = findings.reduce((acc, finding) => {
    acc.legacyCu += finding.compute.legacyCu;
    acc.pTokenCu += finding.compute.pTokenCu;
    acc.savedCu += finding.compute.savedCu;
    return acc;
  }, { legacyCu: 0, pTokenCu: 0, savedCu: 0 });

  const simulation = runDryRun(findings, idlHints);
  return {
    schemaVersion: "0.1.0",
    generatedAt: new Date().toISOString(),
    protocol,
    root: options.root || "uploaded-sources",
    pTokenProfile: {
      name: benchmarkProfile.name,
      status: benchmarkProfile.status,
      note: benchmarkProfile.note
    },
    totals: {
      ...totals,
      savingsPercent: totals.legacyCu ? Math.round((totals.savedCu / totals.legacyCu) * 1000) / 10 : 0,
      callSites: findings.length,
      filesScanned: files.length
    },
    idlHints,
    findings,
    simulation
  };
}

export async function loadBenchmarkProfile(profilePath: string): Promise<BenchmarkProfile> {
  const raw = await fs.readFile(profilePath, "utf8");
  const parsed = JSON.parse(raw) as BenchmarkProfile;
  validateBenchmarkProfile(parsed);
  return parsed;
}

export function runDryRun(findings: Finding[], idlHints: IdlHint[] = []): SimulationReport {
  const divergences: SimulationReport["divergences"] = [];
  for (const finding of findings) {
    if (finding.risk.level === "high") {
      divergences.push({
        findingId: finding.id,
        severity: "review",
        reason: "Authority, signer, or token-program account constraints need manual confirmation."
      });
    }
  }

  if (idlHints.length > 0 && findings.length === 0) {
    divergences.push({
      findingId: "idl-only",
      severity: "review",
      reason: "IDL references the legacy token program but no Rust CPI call site was found."
    });
  }

  return {
    status: divergences.length ? "review_required" : "passed",
    legacyRuns: findings.length,
    pTokenRuns: findings.length,
    divergences
  };
}

export function createReplacementPatch(finding: Pick<Finding, "file" | "line" | "operation">): string {
  const operation = finding.operation;
  const accountHint = operation === "transfer" ? "PTokenTransfer" : `PToken${toPascalCase(operation)}`;
  return [
    `// ${finding.file}:${finding.line}`,
    `let ctx = CpiContext::new(ctx.accounts.p_token_program.to_account_info(), ${accountHint} {`,
    ...replacementAccounts(operation),
    "});",
    `p_token_shim::${operation}(ctx, amount)?;`
  ].join("\n");
}

export async function readJobs(storePath = "data/jobs.json"): Promise<Job[]> {
  try {
    const raw = await fs.readFile(storePath, "utf8");
    return JSON.parse(raw) as Job[];
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") return [];
    throw error;
  }
}

export async function saveJob(manifest: Manifest, storePath = "data/jobs.json", options: SaveJobOptions = {}): Promise<Job> {
  await fs.mkdir(path.dirname(storePath), { recursive: true });
  const jobs = await readJobs(storePath);
  const id = `job_${Date.now()}`;
  const job: Job = {
    id,
    protocol: manifest.protocol,
    createdAt: manifest.generatedAt,
    totals: manifest.totals,
    simulation: manifest.simulation,
    report: buildReportSummary(manifest, id)
  };
  if (options.storeManifest) {
    job.manifest = manifest;
  }
  jobs.unshift(job);
  await fs.writeFile(storePath, JSON.stringify(jobs.slice(0, options.retentionLimit || 25), null, 2));
  return job;
}

export function buildReportSummary(manifest: Manifest, id?: string): ReportSummary {
  const operations: Record<string, number> = {};
  const risks: Record<RiskLevel, number> = { low: 0, medium: 0, high: 0 };
  const fileCounts = new Map<string, number>();

  for (const finding of manifest.findings) {
    operations[finding.operation] = (operations[finding.operation] || 0) + 1;
    risks[finding.risk.level] += 1;
    fileCounts.set(finding.file, (fileCounts.get(finding.file) || 0) + 1);
  }

  return {
    id,
    protocol: manifest.protocol,
    generatedAt: manifest.generatedAt,
    totals: manifest.totals,
    simulation: manifest.simulation,
    operations,
    risks,
    files: Array.from(fileCounts.entries())
      .map(([file, findings]) => ({ file, findings }))
      .sort((a, b) => b.findings - a.findings || a.file.localeCompare(b.file))
      .slice(0, 20)
  };
}

function buildFinding({ file, line, op, snippet, legacyCu, pTokenCu, source }: BuildFindingInput): Finding {
  const id = `${file}:${line}:${op}`.replace(/[^a-zA-Z0-9:_./-]/g, "_");
  const risk = classifyRisk(source, snippet);
  const finding: Finding = {
    id,
    file,
    line,
    operation: op,
    snippet,
    confidence: snippet.includes("token::") || snippet.includes("spl_token") ? "high" : "medium",
    compute: {
      legacyCu,
      pTokenCu,
      savedCu: legacyCu - pTokenCu,
      savingsPercent: Math.round(((legacyCu - pTokenCu) / legacyCu) * 1000) / 10
    },
    risk,
    replacementPatch: ""
  };
  finding.replacementPatch = createReplacementPatch(finding);
  return finding;
}

function classifyRisk(source: string, snippet: string): Finding["risk"] {
  const needsSignerSeeds = /signer|authority|with_signer|seeds/i.test(source);
  const usesTokenProgramAccount = /token_program/i.test(source);
  if (needsSignerSeeds && usesTokenProgramAccount) {
    return { level: "high", reason: "Signer seeds and token program account constraints are present." };
  }
  if (usesTokenProgramAccount) {
    return { level: "medium", reason: "Token program account must be made switchable during rollout." };
  }
  if (/checked/i.test(snippet)) {
    return { level: "medium", reason: "Checked operation requires mint decimal parity validation." };
  }
  return { level: "low", reason: "Straightforward CPI call site." };
}

function collectIdlHints(source: string, file: string, hints: IdlHint[]): void {
  IDL_ACCOUNT_PATTERN.lastIndex = 0;
  if (!IDL_ACCOUNT_PATTERN.test(source)) return;
  hints.push({
    file,
    type: "legacy_token_program_reference",
    message: "IDL contains a legacy SPL Token program account reference."
  });
}

function inferProtocolName(root: string): string {
  return path.basename(root).replace(/[-_]+/g, " ").replace(/\b\w/g, (char) => char.toUpperCase());
}

function toPascalCase(value: string): string {
  return value.split("_").map((part) => part.charAt(0).toUpperCase() + part.slice(1)).join("");
}

function replacementAccounts(operation: Operation): string[] {
  if (operation === "mint_to") {
    return [
      "    mint: ctx.accounts.mint.to_account_info(),",
      "    to: ctx.accounts.destination.to_account_info(),",
      "    authority: ctx.accounts.authority.to_account_info(),"
    ];
  }
  if (operation === "burn") {
    return [
      "    mint: ctx.accounts.mint.to_account_info(),",
      "    from: ctx.accounts.source.to_account_info(),",
      "    authority: ctx.accounts.authority.to_account_info(),"
    ];
  }
  if (operation === "close_account") {
    return [
      "    account: ctx.accounts.account.to_account_info(),",
      "    destination: ctx.accounts.destination.to_account_info(),",
      "    authority: ctx.accounts.authority.to_account_info(),"
    ];
  }
  return [
    "    source: ctx.accounts.source.to_account_info(),",
    "    destination: ctx.accounts.destination.to_account_info(),",
    "    authority: ctx.accounts.authority.to_account_info(),"
  ];
}

function validateBenchmarkProfile(profile: BenchmarkProfile): void {
  const operations: Operation[] = ["transfer", "mint_to", "burn", "approve", "close_account", "initialize_account"];
  for (const operation of operations) {
    const entry = profile.operations?.[operation];
    if (!entry || !Number.isFinite(entry.legacyCu) || !Number.isFinite(entry.pTokenCu)) {
      throw new Error(`Invalid benchmark profile entry for ${operation}`);
    }
  }
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}
