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
}

interface ScanResponse {
  job: Job;
  manifest: Manifest;
}

interface JobsResponse {
  jobs: Job[];
}

const state: {
  manifest: Manifest | null;
  jobs: Job[];
} = {
  manifest: null,
  jobs: []
};

const elements = {
  apiStatus: requiredElement<HTMLElement>("#apiStatus"),
  form: requiredElement<HTMLFormElement>("#scanForm"),
  protocol: requiredElement<HTMLInputElement>("#protocol"),
  apiKey: requiredElement<HTMLInputElement>("#apiKey"),
  projectPath: requiredElement<HTMLInputElement>("#projectPath"),
  sourceFiles: requiredElement<HTMLInputElement>("#sourceFiles"),
  uploadSummary: requiredElement<HTMLElement>("#uploadSummary"),
  sampleButton: requiredElement<HTMLButtonElement>("#sampleButton"),
  scanButton: requiredElement<HTMLButtonElement>("#scanButton"),
  message: requiredElement<HTMLElement>("#message"),
  refreshJobs: requiredElement<HTMLButtonElement>("#refreshJobs"),
  downloadManifest: requiredElement<HTMLButtonElement>("#downloadManifest"),
  reportLink: requiredElement<HTMLAnchorElement>("#reportLink"),
  findingsList: requiredElement<HTMLElement>("#findingsList"),
  simulation: requiredElement<HTMLElement>("#simulation"),
  jobsList: requiredElement<HTMLElement>("#jobsList"),
  template: requiredElement<HTMLTemplateElement>("#findingTemplate"),
  metricCallSites: requiredElement<HTMLElement>("#metricCallSites"),
  metricLegacyCu: requiredElement<HTMLElement>("#metricLegacyCu"),
  metricPtokenCu: requiredElement<HTMLElement>("#metricPtokenCu"),
  metricSavings: requiredElement<HTMLElement>("#metricSavings")
};

elements.form.addEventListener("submit", async (event: SubmitEvent) => {
  event.preventDefault();
  await runScan();
});

elements.sampleButton.addEventListener("click", () => {
  elements.projectPath.value = "sample";
  elements.protocol.value = "Sample Vault";
  elements.sourceFiles.value = "";
  renderUploadSummary(null);
});

elements.sourceFiles.addEventListener("change", () => {
  renderUploadSummary(elements.sourceFiles.files);
});

elements.refreshJobs.addEventListener("click", loadJobs);
elements.downloadManifest.addEventListener("click", downloadManifest);

await checkHealth();
await loadJobs();

async function checkHealth(): Promise<void> {
  try {
    await requestJson<unknown>("/api/health");
    elements.apiStatus.textContent = "API online";
    elements.apiStatus.className = "status-pill ok";
  } catch {
    elements.apiStatus.textContent = "API offline";
    elements.apiStatus.className = "status-pill error";
    showMessage("Could not reach the local API. Start it with npm run dev.", true);
  }
}

async function runScan(): Promise<void> {
  setLoading(true);
  showMessage("Scanning project and building migration manifest...");
  try {
    const payload = {
      protocol: elements.protocol.value.trim(),
      projectPath: elements.projectPath.value.trim()
    };
    const uploadedFiles = await readSelectedFiles(elements.sourceFiles.files);
    const endpoint = uploadedFiles.length ? "/api/scan-sources" : "/api/scan";
    const body = uploadedFiles.length ? { protocol: payload.protocol, files: uploadedFiles } : payload;
    const response = await requestJson<ScanResponse>(endpoint, {
      method: "POST",
      headers: scanHeaders(),
      body: JSON.stringify(body)
    });
    state.manifest = response.manifest;
    renderManifest(response.manifest, response.job);
    await loadJobs();
    showMessage(`Scan complete: ${response.manifest.totals.callSites} call sites found.`);
  } catch (error) {
    showMessage(error instanceof Error ? error.message : "Scan failed. Check the project path and try again.", true);
  } finally {
    setLoading(false);
  }
}

function scanHeaders(): HeadersInit {
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  const key = elements.apiKey.value.trim();
  if (key) headers["X-API-Key"] = key;
  return headers;
}

async function readSelectedFiles(fileList: FileList | null): Promise<Array<{ relative: string; content: string }>> {
  const files = Array.from(fileList || []);
  const supported = new Set([".rs", ".json", ".toml"]);
  const selected = files
    .filter((file) => supported.has(file.name.slice(file.name.lastIndexOf("."))))
    .filter((file) => !ignoredPath(relativePath(file)))
    .slice(0, 500);

  return Promise.all(selected.map(async (file) => ({
    relative: relativePath(file),
    content: await file.text()
  })));
}

function renderUploadSummary(fileList: FileList | null): void {
  const files = Array.from(fileList || []);
  if (!files.length) {
    elements.uploadSummary.textContent = "";
    return;
  }

  const supported = files.filter((file) => [".rs", ".json", ".toml"].includes(file.name.slice(file.name.lastIndexOf("."))) && !ignoredPath(relativePath(file)));
  const totalBytes = supported.reduce((sum, file) => sum + file.size, 0);
  const skipped = files.length - supported.length;
  const parts = [`${formatNumber(supported.length)} files`, `${formatBytes(totalBytes)}`];
  if (skipped > 0) parts.push(`${formatNumber(skipped)} skipped`);
  if (supported.length > 500) parts.push("first 500 files will be scanned");
  elements.uploadSummary.textContent = parts.join(" · ");
}

function relativePath(file: File): string {
  return (file.webkitRelativePath || file.name).replaceAll("\\", "/");
}

function ignoredPath(filePath: string): boolean {
  return filePath.split("/").some((part) => ["node_modules", "target", ".git", "dist", ".next"].includes(part));
}

async function loadJobs(): Promise<void> {
  try {
    const response = await requestJson<JobsResponse>("/api/jobs");
    state.jobs = response.jobs || [];
    renderJobs(state.jobs);
  } catch {
    elements.jobsList.className = "jobs-list empty-state";
    elements.jobsList.innerHTML = "<p>Could not load runs</p><span>Retry after the API is online.</span>";
  }
}

function renderManifest(manifest: Manifest, job?: Job): void {
  renderMetrics(manifest.totals);
  renderFindings(manifest.findings);
  renderSimulation(manifest.simulation);
  elements.downloadManifest.disabled = false;
  if (job?.id) {
    elements.reportLink.href = `/reports/${encodeURIComponent(job.id)}`;
    elements.reportLink.classList.remove("disabled");
    elements.reportLink.removeAttribute("aria-disabled");
  }
}

function renderMetrics(totals: ComputeTotals): void {
  elements.metricCallSites.textContent = formatNumber(totals.callSites);
  elements.metricLegacyCu.textContent = formatNumber(totals.legacyCu);
  elements.metricPtokenCu.textContent = formatNumber(totals.pTokenCu);
  elements.metricSavings.textContent = `${totals.savingsPercent}%`;
}

function renderFindings(findings: Finding[]): void {
  elements.findingsList.className = "list";
  elements.findingsList.innerHTML = "";

  if (!findings.length) {
    elements.findingsList.className = "list empty-state";
    elements.findingsList.innerHTML = "<p>No SPL Token CPI call sites found</p><span>Try another project path or inspect IDL hints.</span>";
    return;
  }

  for (const finding of findings) {
    const node = elements.template.content.cloneNode(true) as DocumentFragment;
    requiredInNode<HTMLElement>(node, "h3").textContent = operationLabel(finding.operation);
    requiredInNode<HTMLElement>(node, ".file-line").textContent = `${finding.file}:${finding.line}`;
    const risk = requiredInNode<HTMLElement>(node, ".risk");
    risk.textContent = finding.risk.level;
    risk.classList.add(finding.risk.level);
    requiredInNode<HTMLElement>(node, ".snippet").textContent = finding.snippet;
    requiredInNode<HTMLElement>(node, ".legacy").textContent = `Legacy ${formatNumber(finding.compute.legacyCu)} CU`;
    requiredInNode<HTMLElement>(node, ".ptoken").textContent = `p-token ${formatNumber(finding.compute.pTokenCu)} CU`;
    requiredInNode<HTMLElement>(node, ".saved").textContent = `Save ${formatNumber(finding.compute.savedCu)} CU`;
    requiredInNode<HTMLElement>(node, ".patch").textContent = finding.replacementPatch;
    elements.findingsList.append(node);
  }
}

function renderSimulation(simulation: SimulationReport): void {
  elements.simulation.className = "simulation";
  const rows = [
    `<div class="simulation-row"><strong>${simulation.status.replace("_", " ")}</strong><span>${simulation.legacyRuns} legacy runs vs ${simulation.pTokenRuns} p-token runs</span></div>`
  ];
  if (simulation.divergences.length) {
    rows.push(...simulation.divergences.map((item) => (
      `<div class="simulation-row"><strong>${escapeHtml(item.severity)}</strong><span>${escapeHtml(item.reason)}</span></div>`
    )));
  } else {
    rows.push('<div class="simulation-row"><strong>No divergence detected</strong><span>The estimator found no behavior-risk blockers.</span></div>');
  }
  elements.simulation.innerHTML = rows.join("");
}

function renderJobs(jobs: Job[]): void {
  if (!jobs.length) {
    elements.jobsList.className = "jobs-list empty-state";
    elements.jobsList.innerHTML = "<p>No runs saved</p><span>Completed scans are stored locally.</span>";
    return;
  }

  elements.jobsList.className = "jobs-list";
  elements.jobsList.innerHTML = jobs.slice(0, 6).map((job) => (
    `<a class="job-row" href="/reports/${encodeURIComponent(job.id)}"><strong>${escapeHtml(job.protocol)}</strong><span>${formatNumber(job.totals.callSites)} call sites · ${job.totals.savingsPercent}% savings</span></a>`
  )).join("");
}

function downloadManifest(): void {
  if (!state.manifest) return;
  const blob = new Blob([JSON.stringify(state.manifest, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `${state.manifest.protocol.toLowerCase().replace(/[^a-z0-9]+/g, "-")}-migration-manifest.json`;
  link.click();
  URL.revokeObjectURL(url);
}

async function requestJson<T>(url: string, options?: RequestInit): Promise<T> {
  const response = await fetch(url, options);
  const payload = await response.json();
  if (!response.ok) throw new Error(payload.error || "Request failed");
  return payload as T;
}

function setLoading(isLoading: boolean): void {
  elements.scanButton.disabled = isLoading;
  elements.scanButton.setAttribute("aria-busy", String(isLoading));
  elements.scanButton.innerHTML = isLoading ? '<span aria-hidden="true">...</span> Scanning' : '<span aria-hidden="true">▶</span> Scan project';
}

function showMessage(text: string, isError = false): void {
  elements.message.textContent = text;
  elements.message.className = isError ? "message error" : "message";
}

function formatNumber(value: number): string {
  return new Intl.NumberFormat("en-US").format(value || 0);
}

function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${Math.round(value / 102.4) / 10} KB`;
  return `${Math.round(value / 1024 / 102.4) / 10} MB`;
}

function operationLabel(operation: string): string {
  return operation.split("_").map((part) => part.charAt(0).toUpperCase() + part.slice(1)).join(" ");
}

function escapeHtml(value: string): string {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

function requiredElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`Missing element: ${selector}`);
  return element;
}

function requiredInNode<T extends Element>(node: ParentNode, selector: string): T {
  const element = node.querySelector<T>(selector);
  if (!element) throw new Error(`Missing template element: ${selector}`);
  return element;
}
