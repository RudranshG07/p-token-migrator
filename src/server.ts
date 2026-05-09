import http, { type IncomingMessage, type ServerResponse } from "node:http";
import { promises as fs } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { getSampleProjectPath, readJobs, saveJob, scanProject } from "./migrator.ts";

interface ScanBody {
  protocol?: string;
  projectPath?: string;
}

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const publicDir = path.resolve(__dirname, "../web");
const port = Number(process.env.PORT || 4173);
const host = process.env.HOST || "127.0.0.1";

const server = http.createServer(async (req: IncomingMessage, res: ServerResponse) => {
  try {
    const url = new URL(req.url || "/", `http://${req.headers.host || `${host}:${port}`}`);

    if (req.method === "GET" && url.pathname === "/api/health") {
      return sendJson(res, { ok: true, sampleProject: getSampleProjectPath() });
    }

    if (req.method === "GET" && url.pathname === "/api/jobs") {
      return sendJson(res, { jobs: await readJobs() });
    }

    if (req.method === "POST" && url.pathname === "/api/scan") {
      const body = await readBody<ScanBody>(req);
      const projectPath = body.projectPath === "sample" || !body.projectPath ? getSampleProjectPath() : body.projectPath;
      const manifest = await scanProject(projectPath, { protocol: body.protocol });
      const job = await saveJob(manifest);
      return sendJson(res, { job, manifest });
    }

    if (req.method === "GET" && url.pathname === "/api/sample") {
      const manifest = await scanProject(getSampleProjectPath(), { protocol: "Sample Vault" });
      return sendJson(res, { manifest });
    }

    return serveStatic(url.pathname, res);
  } catch (error) {
    return sendJson(res, { error: error instanceof Error ? error.message : "Unexpected server error" }, 500);
  }
});

server.listen(port, host, () => {
  console.log(`p-token migrator running at http://${host}:${port}`);
});

async function serveStatic(requestPath: string, res: ServerResponse): Promise<void> {
  if (requestPath === "/app.js") {
    const source = await fs.readFile(path.join(publicDir, "app.ts"), "utf8");
    res.writeHead(200, { "Content-Type": "text/javascript; charset=utf-8" });
    res.end(stripTypeScriptTypes(source, { mode: "strip" }));
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

function contentType(file: string): string {
  const ext = path.extname(file);
  if (ext === ".html") return "text/html; charset=utf-8";
  if (ext === ".css") return "text/css; charset=utf-8";
  if (ext === ".json") return "application/json; charset=utf-8";
  return "application/octet-stream";
}

async function readBody<T>(req: IncomingMessage): Promise<T> {
  const chunks: Buffer[] = [];
  for await (const chunk of req) chunks.push(Buffer.from(chunk));
  const raw = Buffer.concat(chunks).toString("utf8");
  return (raw ? JSON.parse(raw) : {}) as T;
}

function sendJson(res: ServerResponse, payload: unknown, status = 200): void {
  res.writeHead(status, { "Content-Type": "application/json; charset=utf-8" });
  res.end(JSON.stringify(payload, null, 2));
}

function sendText(res: ServerResponse, payload: string, status = 200): void {
  res.writeHead(status, { "Content-Type": "text/plain; charset=utf-8" });
  res.end(payload);
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}
