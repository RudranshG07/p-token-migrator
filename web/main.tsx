import React, { type FormEvent, useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";

interface ComputeTotals {
  legacyCu: number;
  pTokenCu: number;
  savedCu: number;
  savingsPercent: number;
  callSites: number;
  filesScanned: number;
}

interface Finding {
  file: string;
  line: number;
  operation: string;
  snippet: string;
  compute: {
    legacyCu: number;
    pTokenCu: number;
    savedCu: number;
  };
  risk: {
    level: "low" | "medium" | "high";
    reason?: string;
  };
  replacementPatch: string;
}

interface SimulationReport {
  status: string;
  legacyRuns: number;
  pTokenRuns: number;
  divergences: Array<{
    severity: string;
    reason: string;
  }>;
}

interface Manifest {
  protocol: string;
  totals: ComputeTotals;
  findings: Finding[];
  simulation: SimulationReport;
}

interface Job {
  id: string;
  protocol: string;
  totals: ComputeTotals;
  report?: ReportSummary;
}

interface ReportSummary {
  id?: string;
  protocol: string;
  generatedAt: string;
  totals: ComputeTotals;
  simulation: SimulationReport;
  operations: Record<string, number>;
  risks: Record<string, number>;
  files: Array<{ file: string; findings: number }>;
}

interface SourcePayload {
  relative: string;
  content: string;
}

type HealthState = "checking" | "online" | "offline";

const emptyTotals: ComputeTotals = {
  legacyCu: 0,
  pTokenCu: 0,
  savedCu: 0,
  savingsPercent: 0,
  callSites: 0,
  filesScanned: 0
};

function App(): React.ReactElement {
  const reportId = useMemo(() => reportIdFromPath(location.pathname), []);
  return reportId ? <ReportPage reportId={reportId} /> : <ScannerPage />;
}

function ScannerPage(): React.ReactElement {
  const [health, setHealth] = useState<HealthState>("checking");
  const [protocol, setProtocol] = useState("Sample Vault");
  const [apiKey, setApiKey] = useState("");
  const [projectPath, setProjectPath] = useState("sample");
  const [fileList, setFileList] = useState<FileList | null>(null);
  const [manifest, setManifest] = useState<Manifest | null>(null);
  const [job, setJob] = useState<Job | null>(null);
  const [jobs, setJobs] = useState<Job[]>([]);
  const [message, setMessage] = useState("Ready to scan the bundled sample or an uploaded source folder.");
  const [error, setError] = useState("");
  const [isScanning, setIsScanning] = useState(false);

  useEffect(() => {
    void checkHealth(setHealth, setError);
    void loadJobs(setJobs);
  }, []);

  const selectedFiles = useMemo(() => summarizeFiles(fileList), [fileList]);
  const totals = manifest?.totals || emptyTotals;

  async function submitScan(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    setIsScanning(true);
    setError("");
    setMessage("Scanning project and building migration manifest...");

    try {
      const uploadedFiles = await readSelectedFiles(fileList);
      const endpoint = uploadedFiles.length ? "/api/scan-sources" : "/api/scan";
      const body = uploadedFiles.length
        ? { protocol: protocol.trim(), files: uploadedFiles }
        : { protocol: protocol.trim(), projectPath: projectPath.trim() };
      const response = await requestJson<{ job: Job; manifest: Manifest }>(endpoint, {
        method: "POST",
        headers: scanHeaders(apiKey),
        body: JSON.stringify(body)
      });

      setManifest(response.manifest);
      setJob(response.job);
      setMessage(`Scan complete: ${formatNumber(response.manifest.totals.callSites)} call sites found.`);
      await loadJobs(setJobs);
    } catch (scanError) {
      setError(scanError instanceof Error ? scanError.message : "Scan failed.");
      setMessage("");
    } finally {
      setIsScanning(false);
    }
  }

  function useSample(): void {
    setProjectPath("sample");
    setProtocol("Sample Vault");
    setFileList(null);
    const input = document.querySelector<HTMLInputElement>("#sourceFiles");
    if (input) input.value = "";
  }

  return (
    <>
      <Header
        eyebrow="SIMD-0266 migration workspace"
        title="p-token Migration Toolkit"
        action={<Button variant="square" onClick={() => void loadJobs(setJobs)} ariaLabel="Refresh jobs">↻</Button>}
      />

      <main className="shell">
        <section className="hero-band" aria-label="Migration workspace overview">
          <div>
            <p className="stamp">Public-ready scanner</p>
            <h2>Find SPL Token CPI call sites before the p-token rollout.</h2>
          </div>
          <div className="hero-stat">
            <span>Mode</span>
            <strong>{health === "online" ? "API online" : health === "offline" ? "API offline" : "Checking"}</strong>
          </div>
        </section>

        <section className="scan-layout">
          <Panel className="scan-panel">
            <SectionHeading eyebrow="Scanner" title="Run migration analysis" right={<StatusPill state={health} />} />
            <form className="scan-form" onSubmit={submitScan}>
              <Field label="Protocol name" htmlFor="protocol">
                <input id="protocol" type="text" autoComplete="organization" value={protocol} onChange={(event) => setProtocol(event.target.value)} />
              </Field>

              <Field label="API key" htmlFor="apiKey" help="Only required when the deployment protects scan routes.">
                <input id="apiKey" type="password" autoComplete="off" value={apiKey} onChange={(event) => setApiKey(event.target.value)} placeholder="Protected deployments only" />
              </Field>

              <Field label="Source folder" htmlFor="sourceFiles" help="Browser uploads only Rust, JSON, and TOML files.">
                <input
                  id="sourceFiles"
                  type="file"
                  multiple
                  {...{ webkitdirectory: "", directory: "" }}
                  onChange={(event) => setFileList(event.target.files)}
                />
              </Field>
              <p className="upload-summary" aria-live="polite">{selectedFiles}</p>

              <Field label="Server path" htmlFor="projectPath" help="Use sample for the bundled demo. Public deployments should scan uploads.">
                <div className="path-row">
                  <input id="projectPath" type="text" autoComplete="off" value={projectPath} onChange={(event) => setProjectPath(event.target.value)} />
                  <Button type="button" variant="secondary" onClick={useSample}>Use sample</Button>
                </div>
              </Field>

              <div className="form-actions">
                <Button type="submit" variant="primary" disabled={isScanning}>
                  <span aria-hidden="true">{isScanning ? "..." : "▶"}</span>
                  {isScanning ? "Scanning" : "Scan project"}
                </Button>
                {job ? <a className="neo-link" href={`/reports/${encodeURIComponent(job.id)}`}>Open report</a> : null}
              </div>
            </form>
            <Message text={error || message} tone={error ? "error" : "default"} />
          </Panel>

          <div className="metric-stack">
            <MetricCard tone="yellow" label="Call sites" value={formatNumber(totals.callSites)} />
            <MetricCard tone="cyan" label="Legacy CU" value={formatNumber(totals.legacyCu)} />
            <MetricCard tone="pink" label="p-token CU" value={formatNumber(totals.pTokenCu)} />
            <MetricCard tone="lime" label="Estimated savings" value={`${totals.savingsPercent}%`} />
          </div>
        </section>

        <section className="workspace">
          <Panel className="findings-panel">
            <SectionHeading
              eyebrow="Manifest"
              title="Detected call sites"
              right={<ManifestActions manifest={manifest} job={job} />}
            />
            <FindingsList findings={manifest?.findings || []} hasScanned={Boolean(manifest)} />
          </Panel>

          <aside className="side-stack">
            <Panel>
              <SectionHeading eyebrow="Dry-run" title="Simulation report" />
              <SimulationBlock simulation={manifest?.simulation || null} />
            </Panel>
            <Panel>
              <SectionHeading eyebrow="History" title="Recent runs" />
              <JobsList jobs={jobs} />
            </Panel>
          </aside>
        </section>
      </main>
    </>
  );
}

function ReportPage({ reportId }: { reportId: string }): React.ReactElement {
  const [report, setReport] = useState<ReportSummary | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    requestJson<{ report: ReportSummary }>(`/api/reports/${encodeURIComponent(reportId)}`)
      .then((payload) => setReport(payload.report))
      .catch((reportError) => setError(reportError instanceof Error ? reportError.message : "Could not load report."));
  }, [reportId]);

  const totals = report?.totals || emptyTotals;

  return (
    <>
      <Header eyebrow="Public migration report" title={report?.protocol || "Migration Report"} action={<a className="neo-link" href="/">New scan</a>} />
      <main className="shell">
        <section className="metric-stack report-metrics" aria-label="Report metrics">
          <MetricCard tone="yellow" label="Call sites" value={formatNumber(totals.callSites)} />
          <MetricCard tone="cyan" label="Legacy CU" value={formatNumber(totals.legacyCu)} />
          <MetricCard tone="pink" label="p-token CU" value={formatNumber(totals.pTokenCu)} />
          <MetricCard tone="lime" label="Estimated savings" value={`${totals.savingsPercent}%`} />
        </section>

        <section className="workspace">
          <Panel>
            <SectionHeading eyebrow="Breakdown" title="Operations and risk" />
            {error ? <EmptyState title="Report unavailable" text={error} /> : <ReportBreakdown report={report} />}
          </Panel>
          <Panel>
            <SectionHeading eyebrow="Files" title="Most affected files" />
            <ReportFiles report={report} />
          </Panel>
        </section>
      </main>
    </>
  );
}

function Header({ eyebrow, title, action }: { eyebrow: string; title: string; action?: React.ReactNode }): React.ReactElement {
  return (
    <header className="topbar">
      <div>
        <p className="eyebrow">{eyebrow}</p>
        <h1>{title}</h1>
      </div>
      {action}
    </header>
  );
}

function Panel({ children, className = "" }: { children: React.ReactNode; className?: string }): React.ReactElement {
  return <section className={`panel ${className}`}>{children}</section>;
}

function SectionHeading({ eyebrow, title, right }: { eyebrow: string; title: string; right?: React.ReactNode }): React.ReactElement {
  return (
    <div className="section-heading">
      <div>
        <p className="eyebrow">{eyebrow}</p>
        <h2>{title}</h2>
      </div>
      {right ? <div className="heading-action">{right}</div> : null}
    </div>
  );
}

function Field({ label, htmlFor, help, children }: { label: string; htmlFor: string; help?: string; children: React.ReactNode }): React.ReactElement {
  return (
    <div className="field">
      <label htmlFor={htmlFor}>{label}</label>
      {children}
      {help ? <p className="help-text">{help}</p> : null}
    </div>
  );
}

function Button({ children, variant, type = "button", disabled, onClick, ariaLabel }: {
  children: React.ReactNode;
  variant: "primary" | "secondary" | "square";
  type?: "button" | "submit";
  disabled?: boolean;
  onClick?: () => void;
  ariaLabel?: string;
}): React.ReactElement {
  return (
    <button className={`neo-button ${variant}`} type={type} disabled={disabled} onClick={onClick} aria-label={ariaLabel}>
      {children}
    </button>
  );
}

function StatusPill({ state }: { state: HealthState }): React.ReactElement {
  const label = state === "online" ? "API online" : state === "offline" ? "API offline" : "Checking API";
  return <span className={`status-pill ${state}`}>{label}</span>;
}

function MetricCard({ label, value, tone }: { label: string; value: string; tone: "yellow" | "cyan" | "pink" | "lime" }): React.ReactElement {
  return (
    <article className={`metric ${tone}`}>
      <span>{label}</span>
      <strong>{value}</strong>
    </article>
  );
}

function Message({ text, tone }: { text: string; tone: "default" | "error" }): React.ReactElement {
  return <p className={`message ${tone === "error" ? "error" : ""}`} role="status" aria-live="polite">{text}</p>;
}

function ManifestActions({ manifest, job }: { manifest: Manifest | null; job: Job | null }): React.ReactElement {
  return (
    <div className="button-row">
      {job ? <a className="neo-link" href={`/reports/${encodeURIComponent(job.id)}`}>Open report</a> : <span className="disabled-link">Open report</span>}
      <Button variant="secondary" disabled={!manifest} onClick={() => manifest && downloadManifest(manifest)}>Download JSON</Button>
    </div>
  );
}

function FindingsList({ findings, hasScanned }: { findings: Finding[]; hasScanned: boolean }): React.ReactElement {
  if (!hasScanned) return <EmptyState title="No scan yet" text="Run the sample project to see SPL Token CPI findings." />;
  if (!findings.length) return <EmptyState title="No SPL Token CPI call sites found" text="Try another project path or inspect IDL hints." />;

  return (
    <div className="list">
      {findings.map((finding, index) => <FindingCard finding={finding} key={`${finding.file}:${finding.line}:${index}`} />)}
    </div>
  );
}

function FindingCard({ finding }: { finding: Finding }): React.ReactElement {
  return (
    <article className="finding">
      <div className="finding-header">
        <div>
          <h3>{operationLabel(finding.operation)}</h3>
          <p className="file-line">{finding.file}:{finding.line}</p>
        </div>
        <span className={`risk ${finding.risk.level}`}>{finding.risk.level}</span>
      </div>
      <pre className="snippet">{finding.snippet}</pre>
      <div className="cu-row">
        <span>Legacy {formatNumber(finding.compute.legacyCu)} CU</span>
        <span>p-token {formatNumber(finding.compute.pTokenCu)} CU</span>
        <strong>Save {formatNumber(finding.compute.savedCu)} CU</strong>
      </div>
      <details>
        <summary>Shim replacement guidance</summary>
        <pre className="patch">{finding.replacementPatch}</pre>
      </details>
    </article>
  );
}

function SimulationBlock({ simulation }: { simulation: SimulationReport | null }): React.ReactElement {
  if (!simulation) return <EmptyState title="No simulation yet" text="The dry-run report appears after a scan." />;

  return (
    <div className="simulation">
      <InfoRow title={simulation.status.replace("_", " ")} text={`${simulation.legacyRuns} legacy runs vs ${simulation.pTokenRuns} p-token runs`} />
      {simulation.divergences.length
        ? simulation.divergences.map((item, index) => <InfoRow title={item.severity} text={item.reason} key={`${item.severity}:${index}`} />)
        : <InfoRow title="No divergence detected" text="The estimator found no behavior-risk blockers." />}
    </div>
  );
}

function JobsList({ jobs }: { jobs: Job[] }): React.ReactElement {
  if (!jobs.length) return <EmptyState title="No runs saved" text="Completed scans are stored locally." />;

  return (
    <div className="jobs-list">
      {jobs.slice(0, 6).map((item) => (
        <a className="job-row" href={`/reports/${encodeURIComponent(item.id)}`} key={item.id}>
          <strong>{item.protocol}</strong>
          <span>{formatNumber(item.totals.callSites)} call sites · {item.totals.savingsPercent}% savings</span>
        </a>
      ))}
    </div>
  );
}

function ReportBreakdown({ report }: { report: ReportSummary | null }): React.ReactElement {
  if (!report) return <EmptyState title="Loading report" text="Fetching the public scan summary." />;
  const rows = [
    ...Object.entries(report.operations).map(([operation, count]) => ({ title: operationLabel(operation), text: `${formatNumber(count)} call sites` })),
    ...Object.entries(report.risks).map(([risk, count]) => ({ title: `${operationLabel(risk)} risk`, text: `${formatNumber(count)} findings` }))
  ];
  return <div className="list">{rows.map((row) => <InfoRow title={row.title} text={row.text} key={`${row.title}:${row.text}`} />)}</div>;
}

function ReportFiles({ report }: { report: ReportSummary | null }): React.ReactElement {
  if (!report) return <EmptyState title="No files yet" text="Files appear after the report loads." />;
  if (!report.files.length) return <EmptyState title="No files listed" text="This report has no file-level findings." />;

  return (
    <div className="jobs-list">
      {report.files.map((file) => (
        <div className="job-row" key={file.file}>
          <strong>{file.file}</strong>
          <span>{formatNumber(file.findings)} findings</span>
        </div>
      ))}
    </div>
  );
}

function InfoRow({ title, text }: { title: string; text: string }): React.ReactElement {
  return (
    <div className="simulation-row">
      <strong>{title}</strong>
      <span>{text}</span>
    </div>
  );
}

function EmptyState({ title, text }: { title: string; text: string }): React.ReactElement {
  return (
    <div className="empty-state">
      <p>{title}</p>
      <span>{text}</span>
    </div>
  );
}

async function checkHealth(setHealth: (state: HealthState) => void, setError: (message: string) => void): Promise<void> {
  try {
    await requestJson<unknown>("/api/health");
    setHealth("online");
  } catch {
    setHealth("offline");
    setError("Could not reach the API.");
  }
}

async function loadJobs(setJobs: (jobs: Job[]) => void): Promise<void> {
  const response = await requestJson<{ jobs: Job[] }>("/api/jobs");
  setJobs(response.jobs || []);
}

async function requestJson<T>(url: string, options?: RequestInit): Promise<T> {
  const response = await fetch(url, options);
  const payload = await response.json();
  if (!response.ok) throw new Error(payload.error || "Request failed");
  return payload as T;
}

function scanHeaders(apiKey: string): HeadersInit {
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (apiKey.trim()) headers["X-API-Key"] = apiKey.trim();
  return headers;
}

async function readSelectedFiles(fileList: FileList | null): Promise<SourcePayload[]> {
  const files = supportedFiles(fileList).slice(0, 500);
  return Promise.all(files.map(async (file) => ({
    relative: relativePath(file),
    content: await file.text()
  })));
}

function summarizeFiles(fileList: FileList | null): string {
  const files = Array.from(fileList || []);
  if (!files.length) return "";
  const selected = supportedFiles(fileList);
  const bytes = selected.reduce((sum, file) => sum + file.size, 0);
  const skipped = files.length - selected.length;
  const parts = [`${formatNumber(selected.length)} files`, formatBytes(bytes)];
  if (skipped > 0) parts.push(`${formatNumber(skipped)} skipped`);
  if (selected.length > 500) parts.push("first 500 files will be scanned");
  return parts.join(" / ");
}

function supportedFiles(fileList: FileList | null): File[] {
  const supported = new Set([".rs", ".json", ".toml"]);
  return Array.from(fileList || [])
    .filter((file) => supported.has(file.name.slice(file.name.lastIndexOf("."))))
    .filter((file) => !ignoredPath(relativePath(file)));
}

function relativePath(file: File): string {
  return (file.webkitRelativePath || file.name).replaceAll("\\", "/");
}

function ignoredPath(filePath: string): boolean {
  return filePath.split("/").some((part) => ["node_modules", "target", ".git", "dist", ".next"].includes(part));
}

function downloadManifest(manifest: Manifest): void {
  const blob = new Blob([JSON.stringify(manifest, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `${manifest.protocol.toLowerCase().replace(/[^a-z0-9]+/g, "-")}-migration-manifest.json`;
  link.click();
  URL.revokeObjectURL(url);
}

function reportIdFromPath(pathname: string): string {
  if (!pathname.startsWith("/reports/")) return "";
  return pathname.split("/").filter(Boolean).at(-1) || "";
}

function operationLabel(operation: string): string {
  return operation.split("_").map((part) => part.charAt(0).toUpperCase() + part.slice(1)).join(" ");
}

function formatNumber(value: number): string {
  return new Intl.NumberFormat("en-US").format(value || 0);
}

function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${Math.round(value / 102.4) / 10} KB`;
  return `${Math.round(value / 1024 / 102.4) / 10} MB`;
}

const root = document.querySelector("#root");
if (!root) throw new Error("Missing root element.");
createRoot(root).render(<App />);
