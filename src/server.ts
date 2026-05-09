import http, { type IncomingMessage, type ServerResponse } from "node:http";
import { promises as fs } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { getSampleProjectPath, readJobs, saveJob, scanProject, scanSourceFiles, type SourceFile } from "./migrator.ts";

interface ScanBody {
  protocol?: string;
  projectPath?: string;
}

interface ScanSourcesBody {
  protocol?: string;
  files?: SourceFile[];
}

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const publicDir = path.resolve(__dirname, "../web");
const port = Number(process.env.PORT || 4173);
const isProduction = process.env.NODE_ENV === "production";
const host = process.env.HOST || (isProduction ? "0.0.0.0" : "127.0.0.1");
const allowServerPathScan = process.env.ALLOW_SERVER_PATH_SCAN === "1" || !isProduction;
const maxBodyBytes = Number(process.env.MAX_BODY_BYTES || 10 * 1024 * 1024);
const jobStorePath = process.env.JOB_STORE_PATH || "data/jobs.json";
const storeFullManifests = process.env.STORE_FULL_MANIFESTS === "1" || !isProduction;
const apiKey = process.env.API_KEY || "";
const rateLimitWindowMs = Number(process.env.RATE_LIMIT_WINDOW_MS || 60_000);
const rateLimitMax = Number(process.env.RATE_LIMIT_MAX || (isProduction ? 20 : 200));
const jobRetentionLimit = Number(process.env.JOB_RETENTION_LIMIT || 100);
const rateLimitBuckets = new Map<string, { count: number; resetAt: number }>();

export interface RequestPolicyInput {
  method?: string;
  pathname: string;
  client: string;
  authorization?: string;
  apiKeyHeader?: string;
}

export interface RequestPolicyConfig {
  apiKey?: string;
  rateLimitWindowMs: number;
  rateLimitMax: number;
  now: number;
  buckets: Map<string, { count: number; resetAt: number }>;
}

export function createServer(): http.Server {
  return http.createServer(async (req: IncomingMessage, res: ServerResponse) => {
    try {
      setBaseHeaders(res);
      const url = new URL(req.url || "/", `http://${req.headers.host || `${host}:${port}`}`);
      enforceRequestPolicy(req, url);

      if (req.method === "GET" && url.pathname === "/api/health") {
        return sendJson(res, {
          ok: true,
          mode: isProduction ? "production" : "development",
          serverPathScan: allowServerPathScan,
          storeFullManifests,
          authRequired: Boolean(apiKey),
          rateLimit: {
            windowMs: rateLimitWindowMs,
            max: rateLimitMax
          },
          sampleProject: getSampleProjectPath()
        });
      }

      if (req.method === "GET" && url.pathname === "/api/ready") {
        await readJobs(jobStorePath);
        return sendJson(res, { ok: true });
      }

      if (req.method === "GET" && url.pathname === "/api/jobs") {
        const jobs = await readJobs(jobStorePath);
        return sendJson(res, { jobs: storeFullManifests ? jobs : jobs.map(stripStoredManifest) });
      }

      if (req.method === "GET" && url.pathname.startsWith("/api/reports/")) {
        const reportId = decodeURIComponent(url.pathname.replace("/api/reports/", ""));
        const jobs = await readJobs(jobStorePath);
        const job = jobs.find((item) => item.id === reportId);
        if (!job) return sendJson(res, { error: "Report not found" }, 404);
        return sendJson(res, { report: job.report });
      }

      if (req.method === "POST" && url.pathname === "/api/scan") {
        const body = await readBody<ScanBody>(req);
        const projectPath = body.projectPath === "sample" || !body.projectPath ? getSampleProjectPath() : body.projectPath;
        if (projectPath !== getSampleProjectPath() && !allowServerPathScan) {
          return sendJson(res, { error: "Server path scanning is disabled on this deployment. Upload source files instead." }, 403);
        }
        const manifest = await scanProject(projectPath, { protocol: body.protocol });
        const job = await saveJob(manifest, jobStorePath, { storeManifest: storeFullManifests, retentionLimit: jobRetentionLimit });
        return sendJson(res, { job, manifest });
      }

      if (req.method === "POST" && url.pathname === "/api/scan-sources") {
        const body = await readBody<ScanSourcesBody>(req);
        const files = normalizeUploadedFiles(body.files || []);
        if (!files.length) {
          return sendJson(res, { error: "Upload at least one Rust, IDL JSON, or TOML file." }, 400);
        }
        const manifest = await scanSourceFiles(files, { protocol: body.protocol || "Uploaded Project" });
        const job = await saveJob(manifest, jobStorePath, { storeManifest: storeFullManifests, retentionLimit: jobRetentionLimit });
        return sendJson(res, { job, manifest });
      }

      if (req.method === "GET" && url.pathname === "/api/sample") {
        const manifest = await scanProject(getSampleProjectPath(), { protocol: "Sample Vault" });
        return sendJson(res, { manifest });
      }

      return serveStatic(url.pathname, res);
    } catch (error) {
      const status = error instanceof ApiError ? error.status : 500;
      return sendJson(res, { error: error instanceof Error ? error.message : "Unexpected server error" }, status);
    }
  });
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  createServer().listen(port, host, () => {
    console.log(`p-token migrator running at http://${host}:${port}`);
  });
}

async function serveStatic(requestPath: string, res: ServerResponse): Promise<void> {
  if (requestPath === "/app.js") {
    const source = await fs.readFile(path.join(publicDir, "app.ts"), "utf8");
    res.writeHead(200, { "Content-Type": "text/javascript; charset=utf-8" });
    res.end(stripTypeScriptTypes(source, { mode: "strip" }));
    return;
  }

  if (requestPath === "/report.js") {
    const source = await fs.readFile(path.join(publicDir, "report.ts"), "utf8");
    res.writeHead(200, { "Content-Type": "text/javascript; charset=utf-8" });
    res.end(stripTypeScriptTypes(source, { mode: "strip" }));
    return;
  }

  if (requestPath.startsWith("/reports/")) {
    const content = await fs.readFile(path.join(publicDir, "report.html"));
    res.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
    res.end(content);
    return;
  }

  const safePath = requestPath === "/" ? "/index.html" : requestPath;
  const absolute = path.resolve(publicDir, `.${safePath}`);
  if (!absolute.startsWith(publicDir)) {
    return sendText(res, "Not found", 404);
  }

  try {
    const content = await fs.readFile(absolute);
    const type = contentType(absolute);
    res.writeHead(200, { "Content-Type": type });
    res.end(content);
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") return sendText(res, "Not found", 404);
    throw error;
  }
}

function enforceRequestPolicy(req: IncomingMessage, url: URL): void {
  applyRequestPolicy({
    method: req.method,
    pathname: url.pathname,
    client: clientKey(req),
    authorization: req.headers.authorization,
    apiKeyHeader: headerValue(req.headers["x-api-key"])
  }, {
    apiKey,
    rateLimitWindowMs,
    rateLimitMax,
    now: Date.now(),
    buckets: rateLimitBuckets
  });
}

export function applyRequestPolicy(input: RequestPolicyInput, config: RequestPolicyConfig): void {
  if (isStaticRoute(input.pathname) || isPublicApiRoute(input.method, input.pathname)) return;
  enforceRateLimit(input.client, config);
  enforceApiKey(input.authorization, input.apiKeyHeader, config.apiKey || "");
}

function isStaticRoute(pathname: string): boolean {
  return !pathname.startsWith("/api/");
}

function isPublicApiRoute(method: string | undefined, pathname: string): boolean {
  if (method !== "GET") return false;
  return ["/api/health", "/api/ready", "/api/jobs", "/api/sample"].includes(pathname) || pathname.startsWith("/api/reports/");
}

function enforceApiKey(authorization: string | undefined, apiKeyHeader: string | undefined, apiKey: string): void {
  if (!apiKey) return;
  const provided = authorization?.replace(/^Bearer\s+/i, "") || apiKeyHeader || "";
  if (provided !== apiKey) {
    throw new ApiError(401, "Missing or invalid API key.");
  }
}

function enforceRateLimit(client: string, config: RequestPolicyConfig): void {
  const bucket = config.buckets.get(client);
  if (!bucket || bucket.resetAt <= config.now) {
    config.buckets.set(client, { count: 1, resetAt: config.now + config.rateLimitWindowMs });
    return;
  }

  bucket.count += 1;
  if (bucket.count > config.rateLimitMax) {
    throw new ApiError(429, "Rate limit exceeded. Try again later.");
  }
}

function clientKey(req: IncomingMessage): string {
  const forwardedFor = headerValue(req.headers["x-forwarded-for"]);
  if (forwardedFor) return forwardedFor.split(",")[0]?.trim() || "unknown";
  return req.socket.remoteAddress || "unknown";
}

function headerValue(value: string | string[] | undefined): string {
  if (Array.isArray(value)) return value[0] || "";
  return value || "";
}

function normalizeUploadedFiles(files: SourceFile[]): SourceFile[] {
  const supported = new Set([".rs", ".json", ".toml"]);
  return files
    .filter((file) => typeof file.relative === "string" && typeof file.content === "string")
    .map((file) => ({
      relative: file.relative.replaceAll("\\", "/").replace(/^\/+/, ""),
      content: file.content
    }))
    .filter((file) => file.relative && supported.has(path.extname(file.relative)) && !file.relative.includes(".."))
    .slice(0, 500);
}

function contentType(file: string): string {
  const ext = path.extname(file);
  if (ext === ".html") return "text/html; charset=utf-8";
  if (ext === ".css") return "text/css; charset=utf-8";
  if (ext === ".json") return "application/json; charset=utf-8";
  return "application/octet-stream";
}

async function readBody<T>(req: IncomingMessage): Promise<T> {
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const rawChunk of req) {
    const chunk = Buffer.from(rawChunk);
    size += chunk.byteLength;
    if (size > maxBodyBytes) {
      throw new ApiError(413, `Request body exceeds ${maxBodyBytes} bytes`);
    }
    chunks.push(chunk);
  }
  const raw = Buffer.concat(chunks).toString("utf8");
  return (raw ? JSON.parse(raw) : {}) as T;
}

function sendJson(res: ServerResponse, payload: unknown, status = 200): void {
  res.writeHead(status, { "Content-Type": "application/json; charset=utf-8", "Cache-Control": "no-store" });
  res.end(JSON.stringify(payload, null, 2));
}

function sendText(res: ServerResponse, payload: string, status = 200): void {
  res.writeHead(status, { "Content-Type": "text/plain; charset=utf-8" });
  res.end(payload);
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}

function setBaseHeaders(res: ServerResponse): void {
  res.setHeader("X-Content-Type-Options", "nosniff");
  res.setHeader("Referrer-Policy", "no-referrer");
  res.setHeader("Permissions-Policy", "camera=(), microphone=(), geolocation=()");
  res.setHeader("Cross-Origin-Resource-Policy", "same-origin");
}

function stripStoredManifest<T extends { manifest?: unknown }>(job: T): Omit<T, "manifest"> {
  const { manifest: _manifest, ...summary } = job;
  return summary;
}

export class ApiError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}
