interface ReportSummary {
  id?: string;
  protocol: string;
  generatedAt: string;
  totals: {
    legacyCu: number;
    pTokenCu: number;
    savedCu: number;
    savingsPercent: number;
    callSites: number;
    filesScanned: number;
  };
  simulation: {
    status: string;
    divergences: Array<{ severity: string; reason: string }>;
  };
  operations: Record<string, number>;
  risks: Record<string, number>;
  files: Array<{ file: string; findings: number }>;
}

const elements = {
  title: requiredElement<HTMLElement>("#reportTitle"),
  callSites: requiredElement<HTMLElement>("#reportCallSites"),
  legacyCu: requiredElement<HTMLElement>("#reportLegacyCu"),
  pTokenCu: requiredElement<HTMLElement>("#reportPtokenCu"),
  savings: requiredElement<HTMLElement>("#reportSavings"),
  breakdown: requiredElement<HTMLElement>("#reportBreakdown"),
  files: requiredElement<HTMLElement>("#reportFiles")
};

await loadReport();

async function loadReport(): Promise<void> {
  const id = location.pathname.split("/").filter(Boolean).at(-1);
  if (!id) {
    renderError("Report id is missing.");
    return;
  }

  try {
    const response = await fetch(`/api/reports/${encodeURIComponent(id)}`);
    const payload = await response.json();
    if (!response.ok) throw new Error(payload.error || "Report not found");
    renderReport(payload.report as ReportSummary);
  } catch (error) {
    renderError(error instanceof Error ? error.message : "Could not load report.");
  }
}

function renderReport(report: ReportSummary): void {
  elements.title.textContent = report.protocol;
  elements.callSites.textContent = formatNumber(report.totals.callSites);
  elements.legacyCu.textContent = formatNumber(report.totals.legacyCu);
  elements.pTokenCu.textContent = formatNumber(report.totals.pTokenCu);
  elements.savings.textContent = `${report.totals.savingsPercent}%`;

  const operationRows = Object.entries(report.operations).map(([operation, count]) => (
    `<div class="simulation-row"><strong>${escapeHtml(label(operation))}</strong><span>${formatNumber(count)} call sites</span></div>`
  ));
  const riskRows = Object.entries(report.risks).map(([risk, count]) => (
    `<div class="simulation-row"><strong>${escapeHtml(label(risk))} risk</strong><span>${formatNumber(count)} findings</span></div>`
  ));
  elements.breakdown.className = "list";
  elements.breakdown.innerHTML = [...operationRows, ...riskRows].join("");

  if (!report.files.length) {
    elements.files.className = "jobs-list empty-state";
    elements.files.innerHTML = "<p>No files listed</p><span>This report has no file-level findings.</span>";
    return;
  }

  elements.files.className = "jobs-list";
  elements.files.innerHTML = report.files.map((file) => (
    `<div class="job-row"><strong>${escapeHtml(file.file)}</strong><span>${formatNumber(file.findings)} findings</span></div>`
  )).join("");
}

function renderError(message: string): void {
  elements.breakdown.className = "list empty-state";
  elements.breakdown.innerHTML = `<p>Report unavailable</p><span>${escapeHtml(message)}</span>`;
}

function formatNumber(value: number): string {
  return new Intl.NumberFormat("en-US").format(value || 0);
}

function label(value: string): string {
  return value.split("_").map((part) => part.charAt(0).toUpperCase() + part.slice(1)).join(" ");
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
