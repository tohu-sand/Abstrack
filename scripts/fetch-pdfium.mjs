// Downloads the pinned PDFium build from bblanchon/pdfium-binaries into vendor/pdfium/<target>/.
// Usage: node scripts/fetch-pdfium.mjs [target ...]   (default: the current platform)

import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync, existsSync, rmSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const RELEASE = "chromium/8125";
const ASSETS = {
  "win-x64": "9a964261760cff52570aabf038ab346d3a5034dd8b30d0d5fe8861235d356b8b",
  "win-arm64": "d394ba61991fb53f4d87f70988035f337d9e73f2bd2c3c1b966df8c43fcabb05",
  "mac-arm64": "4939bd935ac83ddc09a3a4f038bc43a19cea1660316a3667f26cb07a070e139d",
  "mac-x64": "8a5193535b80c7215d8932f6647ae5097465b6dba2811602c21e120c2af112b0",
  "mac-univ": "0cf06803b9a4acc489fc967a1b05990622ca1cf41fa027ff9cc89eeab9b19279",
};

function currentTarget() {
  const arch = process.arch === "arm64" ? "arm64" : "x64";
  if (process.platform === "win32") return `win-${arch}`;
  if (process.platform === "darwin") return `mac-${arch}`;
  throw new Error(`unsupported platform: ${process.platform}`);
}

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const targets = process.argv.length > 2 ? process.argv.slice(2) : [currentTarget()];

for (const target of targets) {
  const expected = ASSETS[target];
  if (!expected) throw new Error(`unknown target: ${target}`);
  const outDir = join(root, "vendor", "pdfium", target);
  const marker = join(outDir, ".release");
  if (existsSync(marker)) {
    console.log(`${target}: already present`);
    continue;
  }
  const url = `https://github.com/bblanchon/pdfium-binaries/releases/download/${RELEASE}/pdfium-${target}.tgz`;
  console.log(`${target}: downloading ${url}`);
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${target}: HTTP ${res.status}`);
  const data = Buffer.from(await res.arrayBuffer());
  const actual = createHash("sha256").update(data).digest("hex");
  if (actual !== expected) {
    throw new Error(`${target}: sha256 mismatch (expected ${expected}, got ${actual})`);
  }
  rmSync(outDir, { recursive: true, force: true });
  mkdirSync(outDir, { recursive: true });
  const archive = join(outDir, "pdfium.tgz");
  writeFileSync(archive, data);
  execFileSync("tar", ["-xzf", archive, "-C", outDir], { stdio: "inherit" });
  rmSync(archive);
  writeFileSync(marker, `${RELEASE}\n${expected}\n`);
  console.log(`${target}: extracted to ${outDir}`);
}
