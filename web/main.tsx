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

interface MigrationBundle {
  protocol: string;
  generatedAt: string;
  summary: {
    callSites: number;
    savedCu: number;
    savingsPercent: number;
    files: number;
  };
  files: Array<{
    path: string;
    content: string;
  }>;
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
  const pathname = useMemo(() => location.pathname, []);
  const reportId = reportIdFromPath(pathname);
  if (reportId) return <ReportPage reportId={reportId} />;
  if (pathname === "/app") return <ScannerPage />;
  if (pathname === "/docs") return <DocsPage />;
  return <LandingPage />;
}

function LandingPage(): React.ReactElement {
  return (
    <>
      <Header
        eyebrow="Solana token-program analysis"
        title="solana-token-analyzer"
        action={<Nav />}
      />
      <main className="shell">
        <section className="landing-hero">
          <div className="landing-copy">
            <p className="stamp">For Solana protocol teams</p>
            <h2>Static analysis, real CU measurements, and forked-mainnet replay for token-program code paths.</h2>
            <p>
              AST-driven scanner for SPL Token, Token-2022, and p-token CPI sites. LiteSVM-measured compute units. Forked-mainnet replay that diffs your legacy build against a proposed upgrade on real traffic.
            </p>
            <div className="form-actions left">
              <a className="neo-link" href="/app">Launch scanner</a>
              <a className="neo-link secondary" href="/docs">Developer docs</a>
            </div>
          </div>
          <div className="terminal-card" aria-label="CLI preview">
            <div className="terminal-title">CLI workflow</div>
            <pre>{[
              "$ sta scan ./protocol --summary --sarif-out p-token.sarif",
              "files scanned: 128",
              "call sites: 42 (28 anchor_spl::token, 14 token_interface)",
              "legacy CU: 187,400 → target: 8,820 (95.3%)",
              "high-risk findings: 6",
              "",
              "$ replay --rpc $RPC --program YourProgram... \\",
              "    --legacy-so target/legacy.so --new-so target/new.so",
              "25 txs replayed: 23 matched, 2 diverged"
            ].join("\n")}</pre>
          </div>
        </section>

        <section className="proof-strip" aria-label="Product proof points">
          <MetricCard tone="yellow" label="Token programs" value="SPL · 2022 · interface · p-token" />
          <MetricCard tone="cyan" label="CU source" value="Measured (LiteSVM)" />
          <MetricCard tone="pink" label="Replay state" value="Forked mainnet + ALT" />
          <MetricCard tone="lime" label="Distribution" value="CLI · Action · API" />
        </section>

        <section className="feature-grid" aria-label="Product capabilities">
          <FeatureCard title="AST scanner" text="syn-based, resolves use-aliases, full-path calls, and token_interface dispatch. Anchor + native + Token-2022." tone="yellow" />
          <FeatureCard title="Measured CU" text="LiteSVM harness records real compute units per operation. No hardcoded constants. Re-run on every protocol release." tone="cyan" />
          <FeatureCard title="Forked replay" text="Replay recent mainnet txs against forked account state. Address Lookup Tables resolved, upgradeable programs deployed from chain." tone="pink" />
          <FeatureCard title="Two-build diff" text="BYO legacy.so and new.so. Each mainnet tx runs against both. Side-by-side outcomes show exactly what changes." tone="lime" />
        </section>

        <section className="product-section">
          <div className="section-banner">
            <p className="eyebrow">Why it matters</p>
            <h2>Token-program changes touch money movement. They need evidence, not vibes.</h2>
          </div>
          <div className="docs-layout">
            <Panel>
              <SectionHeading eyebrow="Risk" title="What the analyzer surfaces" />
              <div className="step-list">
                <InfoRow title="Token-program version pin" text="Legacy SPL pins, Token-2022 references, and token_interface dispatch sites — flagged with the runtime implication." />
                <InfoRow title="Signer and authority paths" text="PDA seeds, with_signer chains, and program-account constraints that need manual confirmation under a new program." />
                <InfoRow title="Checked operations" text="Calls that depend on mint decimal parity — the silent breakage class on token-program swaps." />
              </div>
            </Panel>
            <Panel>
              <SectionHeading eyebrow="Outputs" title="What teams get" />
              <div className="step-list">
                <InfoRow title="Migration manifest" text="JSON with every finding, file, line, risk, and measured CU delta. SARIF available for GitHub code scanning." />
                <InfoRow title="Replay diff report" text="Per-tx side-by-side: mainnet vs replay vs new build. Divergence list scoped to success / CU / error / log count." />
                <InfoRow title="GitHub Action" text="uses: RudyG07/p-token-migrator@v0 — drop-in analyzer + SARIF upload + optional fail-on-review gate." />
              </div>
            </Panel>
          </div>
        </section>

        <section className="product-section">
          <div className="section-banner cyan">
            <p className="eyebrow">Three surfaces, one engine</p>
            <h2>CLI for developers. GitHub Action for CI. Dashboard for product review.</h2>
          </div>
          <div className="workflow-grid">
            <FeatureCard title="sta CLI" text="cargo install or download the release binary. sta scan, sta scan-sources, --sarif-out, --fail-on-review, --profile-path." tone="yellow" />
            <FeatureCard title="GitHub Action" text="Composite action wraps the engine. Outputs call-sites, saved-cu, high-risk-count. SARIF feeds GitHub code scanning." tone="cyan" />
            <FeatureCard title="Dashboard" text="Upload a project folder. View aggregated CU savings, per-file findings, and shareable reports. Backed by the same Rust engine." tone="pink" />
          </div>
        </section>

        <section className="cta-band">
          <div>
            <p className="eyebrow">Try it</p>
            <h2>Start with the sample vault, then scan a real protocol.</h2>
          </div>
          <div className="form-actions">
            <a className="neo-link" href="/app">Open scanner</a>
            <a className="neo-link secondary" href="/docs">Read docs</a>
          </div>
        </section>
      </main>
    </>
  );
}

function DocsPage(): React.ReactElement {
  return (
    <>
      <Header eyebrow="Developer docs" title="solana-token-analyzer docs" action={<Nav />} />
      <main className="shell">
        <section className="docs-hero">
          <div>
            <p className="stamp">Docs</p>
            <h2>Everything needed to scan, measure, replay, and gate token-program changes.</h2>
            <p className="body-copy">The platform is three Rust binaries (sta, cu-bench, replay) plus a TypeScript dashboard server. CLI is the source of truth; the dashboard and GitHub Action both shell out to the same engine.</p>
          </div>
          <nav className="docs-toc" aria-label="Docs sections">
            <a href="#quickstart">Quickstart</a>
            <a href="#cli">CLI</a>
            <a href="#action">GitHub Action</a>
            <a href="#api">API</a>
            <a href="#deployment">Deployment</a>
            <a href="#outputs">Outputs</a>
            <a href="#limits">Limits</a>
          </nav>
        </section>

        <section id="quickstart" className="doc-block">
          <SectionHeading eyebrow="Quickstart" title="Run the dashboard locally" />
          <div className="docs-layout">
            <Panel>
              <h3 className="doc-title">Install and start</h3>
              <CodeBlock lines={[
                "npm install",
                "npm run dev    # builds the engine + frontend, starts server",
                "",
                "# open",
                "http://127.0.0.1:4173"
              ]} />
            </Panel>
            <Panel>
              <h3 className="doc-title">Routes</h3>
              <div className="step-list">
                <InfoRow title="/" text="Landing page." />
                <InfoRow title="/app" text="Scanner dashboard." />
                <InfoRow title="/docs" text="Developer documentation." />
                <InfoRow title="/reports/:id" text="Public scan report." />
              </div>
            </Panel>
          </div>
        </section>

        <section id="cli" className="doc-block">
          <SectionHeading eyebrow="CLI" title="Three binaries, one engine" />
          <div className="docs-layout">
            <Panel>
              <h3 className="doc-title">sta — static analyzer</h3>
              <CodeBlock lines={[
                "sta scan ./protocol --summary",
                "sta scan ./protocol --out manifest.json \\",
                "    --sarif-out p-token.sarif",
                "sta scan ./protocol --profile-path \\",
                "    profiles/measured-spl-token.json",
                "sta scan ./protocol --fail-on-review",
                "",
                "# Or stream sources via stdin:",
                "echo '{\"protocol\":\"x\",\"files\":[...]}' | sta scan-sources"
              ]} />
            </Panel>
            <Panel>
              <h3 className="doc-title">cu-bench — measure compute units</h3>
              <CodeBlock lines={[
                "cu-bench --out profiles/measured-spl-token.json",
                "cu-bench --include-token-2022 \\",
                "    --out profiles/measured-spl-token.json",
                "",
                "# Then sta picks it up via --profile-path."
              ]} />
            </Panel>
          </div>
          <div className="docs-layout">
            <Panel>
              <h3 className="doc-title">replay — single-build mode</h3>
              <CodeBlock lines={[
                "replay --rpc https://your.rpc.url/ \\",
                "    --program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA \\",
                "    --limit 25 --out replay.json"
              ]} />
            </Panel>
            <Panel>
              <h3 className="doc-title">replay — two-build diff</h3>
              <CodeBlock lines={[
                "replay --rpc $RPC --program YourProgram... \\",
                "    --legacy-so target/legacy.so \\",
                "    --new-so target/new.so \\",
                "    --limit 25 --out diff.json",
                "",
                "# Each tx runs against both builds.",
                "# legacyVsNew[] in the report = the diff."
              ]} />
            </Panel>
          </div>
        </section>

        <section id="action" className="doc-block">
          <SectionHeading eyebrow="GitHub Action" title="Drop-in CI scan" />
          <div className="docs-layout">
            <Panel>
              <h3 className="doc-title">Minimum workflow</h3>
              <CodeBlock lines={[
                "- uses: RudyG07/p-token-migrator@v0",
                "  with:",
                "    project-path: .",
                "    sarif-out: p-token.sarif",
                "    fail-on-review: 'false'",
                "",
                "- uses: github/codeql-action/upload-sarif@v3",
                "  with:",
                "    sarif_file: p-token.sarif"
              ]} />
            </Panel>
            <Panel>
              <h3 className="doc-title">Exposed outputs</h3>
              <div className="step-list">
                <InfoRow title="call-sites" text="Total CPI call sites detected." />
                <InfoRow title="saved-cu" text="Aggregate CU saved (legacy − target)." />
                <InfoRow title="savings-percent" text="Percentage savings across the scan." />
                <InfoRow title="high-risk-count" text="Findings tagged for manual review." />
                <InfoRow title="manifest-path" text="Path to the written manifest JSON." />
              </div>
            </Panel>
          </div>
        </section>

        <section id="api" className="doc-block">
          <SectionHeading eyebrow="API" title="Integrate scans into external tools" />
          <div className="docs-layout">
            <Panel>
              <h3 className="doc-title">Upload source files</h3>
              <CodeBlock lines={[
                "POST /api/scan-sources",
                "Content-Type: application/json",
                "",
                "{",
                "  \"protocol\": \"My Protocol\",",
                "  \"files\": [",
                "    { \"relative\": \"programs/vault/src/lib.rs\", \"content\": \"...\" }",
                "  ]",
                "}"
              ]} />
            </Panel>
            <Panel>
              <h3 className="doc-title">Read reports and health</h3>
              <CodeBlock lines={[
                "GET /api/health",
                "GET /api/ready",
                "GET /api/jobs",
                "GET /api/reports/:id",
                "GET /reports/:id"
              ]} />
            </Panel>
          </div>
        </section>

        <section id="deployment" className="doc-block">
          <SectionHeading eyebrow="Deployment" title="Production checklist" />
          <div className="docs-layout">
            <Panel>
              <h3 className="doc-title">Build and start</h3>
              <CodeBlock lines={[
                "npm ci",
                "npm run typecheck",
                "npm run build",
                "NODE_ENV=production HOST=0.0.0.0 npm run start"
              ]} />
            </Panel>
            <Panel>
              <h3 className="doc-title">Required production env</h3>
              <CodeBlock lines={[
                "ALLOW_SERVER_PATH_SCAN=0",
                "STORE_FULL_MANIFESTS=0",
                "JOB_STORE_PATH=/app/data/jobs.json",
                "RATE_LIMIT_MAX=20",
                "MAX_BODY_BYTES=10485760"
              ]} />
            </Panel>
          </div>
        </section>

        <section id="outputs" className="doc-block">
          <SectionHeading eyebrow="Outputs" title="What the engine produces" />
          <div className="feature-grid docs-feature-grid">
            <FeatureCard title="Manifest" text="Full JSON output with totals, findings, measured CU per operation, risk classification, and per-finding replacement snippets." tone="yellow" />
            <FeatureCard title="SARIF" text="GitHub code-scanning compatible report. Inline PR annotations and security-tab integration via codeql-action/upload-sarif." tone="cyan" />
            <FeatureCard title="Replay report" text="Per-tx outcomes with mainnet vs replay vs new build, and an explicit legacyVsNew divergence list." tone="pink" />
            <FeatureCard title="Public report" text="Shareable summary with CU totals, operation counts, risk breakdown, and affected files." tone="lime" />
            <FeatureCard title="Action outputs" text="call-sites, saved-cu, savings-percent, high-risk-count, manifest-path for downstream workflow steps." tone="yellow" />
          </div>
        </section>

        <section id="limits" className="doc-block">
          <SectionHeading eyebrow="Limits" title="What's honest about the current state" />
          <Panel>
            <div className="step-list">
              <InfoRow title="Single-file AST" text="The syn-based scanner is sound within one file. Cross-file call-graph resolution (helper wrappers calling token CPIs) is the next scanner improvement." />
              <InfoRow title="Replay signers" text="Non-payer signers can't be impersonated; programs gating on a non-payer signer will fail in replay. Surfaced as divergence signal." />
              <InfoRow title="Historical state" text="RPC returns finalized account state, not the exact pre-tx slot. Drift is surfaced as a per-tx divergence, not a fatal error." />
              <InfoRow title="p-token measurements" text="cu-bench currently emits a 96% reduction placeholder for p-token numbers. Real measurements require a canonical p-token program build." />
              <InfoRow title="Storage" text="The default JSON job store is suitable for self-hosted deployments. Postgres backing is on the roadmap." />
            </div>
          </Panel>
        </section>
      </main>
    </>
  );
}

function ScannerPage(): React.ReactElement {
  const [health, setHealth] = useState<HealthState>("checking");
  const [protocol, setProtocol] = useState("Sample Vault");
  const [apiKey, setApiKey] = useState("");
  const [projectPath, setProjectPath] = useState("sample");
  const [fileList, setFileList] = useState<FileList | null>(null);
  const [manifest, setManifest] = useState<Manifest | null>(null);
  const [bundle, setBundle] = useState<MigrationBundle | null>(null);
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
      const response = await requestJson<{ job: Job; manifest: Manifest; codegen: MigrationBundle }>(endpoint, {
        method: "POST",
        headers: scanHeaders(apiKey),
        body: JSON.stringify(body)
      });

      setManifest(response.manifest);
      setBundle(response.codegen);
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
        eyebrow="Scanner workspace"
        title="solana-token-analyzer"
        action={<div className="header-actions"><Nav /><Button variant="square" onClick={() => void loadJobs(setJobs)} ariaLabel="Refresh jobs">↻</Button></div>}
      />

      <main className="shell">
        <section className="hero-band" aria-label="Scanner overview">
          <div>
            <p className="stamp">Engine-backed scanner</p>
            <h2>Find SPL Token, Token-2022, and token_interface call sites with measured CU deltas.</h2>
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
              right={<ManifestActions manifest={manifest} bundle={bundle} job={job} />}
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
      <Header eyebrow="Public migration report" title={report?.protocol || "Migration Report"} action={<Nav />} />
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

function Nav(): React.ReactElement {
  return (
    <nav className="nav-links" aria-label="Primary">
      <a href="/">Home</a>
      <a href="/app">Scanner</a>
      <a href="/docs">Docs</a>
    </nav>
  );
}

function Panel({ children, className = "" }: { children: React.ReactNode; className?: string }): React.ReactElement {
  return <section className={`panel ${className}`}>{children}</section>;
}

function FeatureCard({ title, text, tone }: { title: string; text: string; tone: "yellow" | "cyan" | "pink" | "lime" }): React.ReactElement {
  return (
    <article className={`feature-card ${tone}`}>
      <h3>{title}</h3>
      <p>{text}</p>
    </article>
  );
}

function CodeBlock({ lines }: { lines: string[] }): React.ReactElement {
  return <pre className="code-block">{lines.join("\n")}</pre>;
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

function ManifestActions({ manifest, bundle, job }: { manifest: Manifest | null; bundle: MigrationBundle | null; job: Job | null }): React.ReactElement {
  return (
    <div className="button-row">
      {job ? <a className="neo-link" href={`/reports/${encodeURIComponent(job.id)}`}>Open report</a> : <span className="disabled-link">Open report</span>}
      <Button variant="secondary" disabled={!manifest} onClick={() => manifest && downloadManifest(manifest)}>Download JSON</Button>
      <Button variant="secondary" disabled={!bundle} onClick={() => bundle && downloadBundle(bundle)}>Download bundle</Button>
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

function downloadBundle(bundle: MigrationBundle): void {
  const blob = new Blob([JSON.stringify(bundle, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `${bundle.protocol.toLowerCase().replace(/[^a-z0-9]+/g, "-")}-migration-bundle.json`;
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
