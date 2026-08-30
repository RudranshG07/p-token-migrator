import { spawn } from "node:child_process";
import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import type { BenchmarkProfile, Manifest, SourceFile } from "./migrator.ts";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..");

export interface EngineScanOptions {
  protocol?: string;
  benchmarkProfile?: BenchmarkProfile;
}

export interface EngineScanSourcesOptions extends EngineScanOptions {
  root?: string;
}

export async function engineScanProject(
  projectPath: string,
  options: EngineScanOptions = {}
): Promise<Manifest> {
  const args = ["scan", projectPath];
  if (options.protocol) {
    args.push("--protocol", options.protocol);
  }
  let profilePath: string | undefined;
  if (options.benchmarkProfile) {
    profilePath = await writeTempProfile(options.benchmarkProfile);
    args.push("--profile-path", profilePath);
  }
  try {
    return await runEngine(args);
  } finally {
    if (profilePath) await safeUnlink(profilePath);
  }
}

export async function engineScanSources(
  files: SourceFile[],
  options: EngineScanSourcesOptions = {}
): Promise<Manifest> {
  const stdin = JSON.stringify({
    protocol: options.protocol,
    files,
    profile: options.benchmarkProfile,
    root: options.root,
  });
  return runEngine(["scan-sources"], stdin);
}

async function runEngine(args: string[], stdinPayload?: string): Promise<Manifest> {
  const bin = await resolveBinary();
  return new Promise<Manifest>((resolve, reject) => {
    const child = spawn(bin, args, {
      cwd: repoRoot,
      stdio: ["pipe", "pipe", "pipe"],
    });
    const stdoutChunks: Buffer[] = [];
    const stderrChunks: Buffer[] = [];
    child.stdout.on("data", (chunk) => stdoutChunks.push(Buffer.from(chunk)));
    child.stderr.on("data", (chunk) => stderrChunks.push(Buffer.from(chunk)));
    child.on("error", (error) => reject(error));
    child.on("close", (code) => {
      if (code !== 0) {
        const stderr = Buffer.concat(stderrChunks).toString("utf8").trim();
        reject(new Error(`sta exited with code ${code}: ${stderr || "<no stderr>"}`));
        return;
      }
      try {
        const raw = Buffer.concat(stdoutChunks).toString("utf8");
        resolve(JSON.parse(raw) as Manifest);
      } catch (error) {
        reject(error instanceof Error ? error : new Error(String(error)));
      }
    });
    if (stdinPayload !== undefined) {
      child.stdin.write(stdinPayload);
    }
    child.stdin.end();
  });
}

let cachedBinaryPath: string | undefined;

async function resolveBinary(): Promise<string> {
  if (cachedBinaryPath) return cachedBinaryPath;
  const override = process.env.STA_BIN;
  const exe = process.platform === "win32" ? "sta.exe" : "sta";
  const candidates = [
    override,
    path.join(repoRoot, "target", "release", exe),
    path.join(repoRoot, "target", "debug", exe),
  ].filter((value): value is string => Boolean(value));

  for (const candidate of candidates) {
    if (await fileExists(candidate)) {
      cachedBinaryPath = candidate;
      return candidate;
    }
  }
  throw new Error(
    `solana-token-analyzer binary not found. Build it with \`cargo build -p solana-token-analyzer\` or set STA_BIN. Checked: ${candidates.join(", ")}`
  );
}

async function fileExists(target: string): Promise<boolean> {
  try {
    await fs.access(target);
    return true;
  } catch {
    return false;
  }
}

async function writeTempProfile(profile: BenchmarkProfile): Promise<string> {
  const baseDir = path.join(repoRoot, "data");
  await fs.mkdir(baseDir, { recursive: true });
  const dir = await fs.mkdtemp(path.join(baseDir, ".profile-"));
  const file = path.join(dir, "profile.json");
  await fs.writeFile(file, JSON.stringify(profile));
  return file;
}

async function safeUnlink(target: string): Promise<void> {
  try {
    await fs.rm(path.dirname(target), { recursive: true, force: true });
  } catch {
    // best-effort
  }
}
