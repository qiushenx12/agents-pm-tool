// 构建前端产物，并在输入没有变化时复用现有 dist。
// dist 会被 rust-embed 在 Rust 编译期读取；避免无意义地重写文件可以避免主程序重复链接。

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const distDir = path.join(root, "dist");
const cacheDir = path.join(root, "node_modules", ".cache");
const stampPath = path.join(cacheDir, "agents-pm-tool-frontend.json");

function walkFiles(dir) {
  if (!existsSync(dir)) {
    return [];
  }

  const files = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const entryPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...walkFiles(entryPath));
    } else if (entry.isFile()) {
      files.push(entryPath);
    }
  }
  return files;
}

function inputFiles() {
  const fixedFiles = [
    "package.json",
    "package-lock.json",
    "index.html",
    "config.html",
    "vite.config.ts",
    "tsconfig.json",
    "tsconfig.node.json",
    "scripts/build-frontend.mjs",
  ].map((relativePath) => path.join(root, relativePath));

  return fixedFiles
    .filter((filePath) => existsSync(filePath))
    .concat(walkFiles(path.join(root, "src")))
    .concat(walkFiles(path.join(root, "public")))
    .sort((left, right) => left.localeCompare(right));
}

function relevantEnvironment() {
  return Object.keys(process.env)
    .filter((name) => name.startsWith("VITE_") || name.startsWith("TAURI_ENV_"))
    .sort()
    .map((name) => `${name}=${process.env[name] ?? ""}`)
    .join("\n");
}

function fingerprint(files) {
  const hash = createHash("sha256");
  hash.update(`environment=${relevantEnvironment()}\n`);
  for (const filePath of files) {
    const relativePath = path
      .relative(root, filePath)
      .split(path.sep)
      .join("/");
    hash.update(`${relativePath}\0`);
    hash.update(readFileSync(filePath));
  }
  return hash.digest("hex");
}

function readStamp() {
  try {
    return JSON.parse(readFileSync(stampPath, "utf8"));
  } catch {
    return null;
  }
}

const currentFingerprint = fingerprint(inputFiles());
const stamp = readStamp();
const hasDist = existsSync(path.join(distDir, "index.html")) && existsSync(path.join(distDir, "config.html"));

if (hasDist && stamp?.schemaVersion === 1 && stamp?.fingerprint === currentFingerprint) {
  console.log("前端源码未变化，跳过 vue-tsc/vite build。");
  process.exit(0);
}

console.log("前端源码或构建环境已变化，执行 vue-tsc/vite build……");
if (process.platform === "win32") {
  execFileSync(
    process.env.ComSpec || "cmd.exe",
    ["/d", "/s", "/c", "npm run build:frontend:uncached"],
    { cwd: root, stdio: "inherit" },
  );
} else {
  execFileSync("npm", ["run", "build:frontend:uncached"], {
    cwd: root,
    stdio: "inherit",
  });
}

mkdirSync(cacheDir, { recursive: true });
writeFileSync(
  stampPath,
  `${JSON.stringify(
    {
      schemaVersion: 1,
      fingerprint: fingerprint(inputFiles()),
      builtAt: new Date().toISOString(),
    },
    null,
    2,
  )}\n`,
  "utf8",
);
