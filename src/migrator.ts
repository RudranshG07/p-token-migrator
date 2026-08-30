import { promises as fs } from "node:fs";
import path from "node:path";
import { engineScanProject, engineScanSources } from "./engine.ts";

// =============================================================================
// Public types — mirror crates/solana-token-analyzer's manifest.rs JSON shape.
// =============================================================================

export type Operation =
  | "transfer"
  | "mint_to"
  | "burn"
  | "approve"
  | "close_account"
  | "initialize_account";

export type Confidence = "high" | "medium";
export type RiskLevel = "low" | "medium" | "high";
export type SimulationStatus = "passed" | "review_required";
export type PatternKind = "anchor_cpi" | "spl_instruction" | "token_interface";
export type TokenProgram = "spl_token" | "spl_token_2022" | "token_interface" | "unknown";

export interface BenchmarkProfile {
  name: string;
  status: string;
  note: string;
  operations: Record<string, { legacyCu: number; pTokenCu: number }>;
}

export interface SourceFile {
  relative: string;
  content: string;
}

export interface IdlHint {
  file: string;
  type: string;
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
  tokenProgram?: TokenProgram;
  patternKind?: PatternKind;
}

export interface SimulationReport {
  status: SimulationStatus;
  mode: string;
  legacyRuns: number;
  pTokenRuns: number;
  divergences: Array<{ findingId: string; severity: string; reason: string }>;
}

export interface MilestoneStatus {
  name: string;
  status: "complete" | "mvp" | "blocked";
  evidence: string;
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
  milestones: MilestoneStatus[];
}

export interface ScanOptions {
  protocol?: string;
  benchmarkProfile?: BenchmarkProfile;
}

export interface SaveJobOptions {
  storeManifest?: boolean;
  retentionLimit?: number;
}

export interface ReportSummary {
  id?: string;
  protocol: string;
  generatedAt: string;
  totals: Manifest["totals"];
  simulation: SimulationReport;
  operations: Record<string, number>;
  risks: Record<RiskLevel, number>;
  files: Array<{ file: string; findings: number }>;
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

// =============================================================================
// Bundled estimator profile — mirrors `default_profile()` in the engine.
// Kept here so the dashboard can display + override defaults without spawning
// the binary. Engine treats this profile as the source of truth at scan time.
// =============================================================================

export const DEFAULT_BENCHMARK_PROFILE: BenchmarkProfile = {
  name: "simd-0266-estimator",
  status: "pre-mainnet-estimate",
  note: "CU estimates are bundled placeholders until p-token interfaces and a measured profile are available.",
  operations: {
    transfer: { legacyCu: 5200, pTokenCu: 220 },
    mint_to: { legacyCu: 6100, pTokenCu: 260 },
    burn: { legacyCu: 5700, pTokenCu: 250 },
    approve: { legacyCu: 4800, pTokenCu: 210 },
    close_account: { legacyCu: 5000, pTokenCu: 240 },
    initialize_account: { legacyCu: 7400, pTokenCu: 360 },
  },
};

// =============================================================================
// Scanner — delegates to the Rust analyzer binary via src/engine.ts.
// =============================================================================

export function getSampleProjectPath(): string {
  return path.resolve("samples/anchor-token-vault");
}

export async function scanProject(
  projectPath: string,
  options: ScanOptions = {}
): Promise<Manifest> {
  return engineScanProject(projectPath, {
    protocol: options.protocol,
    benchmarkProfile: options.benchmarkProfile,
  });
}

export async function scanSourceFiles(
  files: SourceFile[],
  options: ScanOptions = {}
): Promise<Manifest> {
  return engineScanSources(files, {
    protocol: options.protocol,
    benchmarkProfile: options.benchmarkProfile,
  });
}

export async function loadBenchmarkProfile(
  profilePath: string
): Promise<BenchmarkProfile> {
  const raw = await fs.readFile(profilePath, "utf8");
  return JSON.parse(raw) as BenchmarkProfile;
}

// =============================================================================
// Job persistence (unchanged — backed by a JSON file on disk for now).
// =============================================================================

export async function readJobs(storePath = "data/jobs.json"): Promise<Job[]> {
  try {
    const raw = await fs.readFile(storePath, "utf8");
    return JSON.parse(raw) as Job[];
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") return [];
    throw error;
  }
}

export async function saveJob(
  manifest: Manifest,
  storePath = "data/jobs.json",
  options: SaveJobOptions = {}
): Promise<Job> {
  await fs.mkdir(path.dirname(storePath), { recursive: true });
  const jobs = await readJobs(storePath);
  const id = `job_${Date.now()}`;
  const job: Job = {
    id,
    protocol: manifest.protocol,
    createdAt: manifest.generatedAt,
    totals: manifest.totals,
    simulation: manifest.simulation,
    report: buildReportSummary(manifest, id),
  };
  if (options.storeManifest) {
    job.manifest = manifest;
  }
  jobs.unshift(job);
  await fs.writeFile(
    storePath,
    JSON.stringify(jobs.slice(0, options.retentionLimit || 25), null, 2)
  );
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
      .slice(0, 20),
  };
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}
