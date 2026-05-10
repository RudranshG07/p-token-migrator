import { promises as fs } from "node:fs";
import path from "node:path";
import { build } from "esbuild";

const root = process.cwd();
const sourceDir = path.join(root, "web");
const outDir = path.join(root, "web-dist");

await fs.rm(outDir, { recursive: true, force: true });
await fs.mkdir(path.join(outDir, "assets"), { recursive: true });

await build({
  entryPoints: [path.join(sourceDir, "main.tsx")],
  outfile: path.join(outDir, "assets/main.js"),
  bundle: true,
  format: "esm",
  platform: "browser",
  jsx: "automatic",
  minify: true,
  sourcemap: true,
  target: ["es2022"],
  logLevel: "info"
});

await Promise.all([
  fs.copyFile(path.join(sourceDir, "index.html"), path.join(outDir, "index.html")),
  fs.copyFile(path.join(sourceDir, "styles.css"), path.join(outDir, "styles.css"))
]);
