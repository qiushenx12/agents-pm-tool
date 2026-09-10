// 构建 pm-cli sidecar。
//
// Tauri 的 beforeBuildCommand 每次打包都会执行，但 pm-cli 通常没有变化。
// 用源文件指纹避免无意义的 release 链接；主程序交给后续 Tauri 构建，避免重复链接。

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import {
  existsSync,
  readdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const manifestArg = path.join("src-tauri", "Cargo.toml");
const binariesDir = path.join(root, "src-tauri", "binaries");
const stampPath = path.join(binariesDir, ".pm-cli-build.json");
const forceBuild = process.env.FORCE_CLI_BUILD === "1";

function rustFiles(dir) {
  if (!existsSync(dir)) {
    return [];
  }

  const files = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const entryPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...rustFiles(entryPath));
    } else if (entry.isFile() && entry.name.endsWith(".rs")) {
      files.push(entryPath);
    }
  }
  return files;
}

function inputFiles() {
  const fixedFiles = [
    "src-tauri/Cargo.toml",
    "src-tauri/Cargo.lock",
    "src-tauri/build.rs",
    "scripts/build-cli.mjs",
    "scripts/copy-cli.mjs",
    "scripts/package-cli-skill.mjs",
    "pm-cli-skill/SKILL.md",
    ".cargo/config.toml",
    ".cargo/config",
    "src-tauri/.cargo/config.toml",
    "src-tauri/.cargo/config",
  ].map((relativePath) => path.join(root, relativePath));

  return fixedFiles
    .filter((filePath) => existsSync(filePath))
    .concat(rustFiles(path.join(root, "src-tauri", "src")))
    .sort((left, right) => left.localeCompare(right));
}

function rustcInfo() {
  return execFileSync("rustc", ["-vV"], {
    cwd: root,
    encoding: "utf8",
  });
}

function targetTriple(info) {
  const match = info.match(/^host:\s*(\S+)$/m);
  if (!match) {
    throw new Error("无法从 rustc -vV 读取 host target");
  }
  return match[1];
}

function fingerprint(files, info, target) {
  const hash = createHash("sha256");
  hash.update(`target=${target}\n`);
  hash.update(`rustc=${info}\n`);
  hash.update(`RUSTFLAGS=${process.env.RUSTFLAGS ?? ""}\n`);
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

const compilerInfo = rustcInfo();
const target = targetTriple(compilerInfo);
const sidecarName = `pm-cli-${target}${process.platform === "win32" ? ".exe" : ""}`;
const sidecarPath = path.join(binariesDir, sidecarName);
const currentFingerprint = fingerprint(inputFiles(), compilerInfo, target);
const stamp = readStamp();

const canSkipBuild =
  !forceBuild &&
  existsSync(sidecarPath) &&
  stamp?.schemaVersion === 1 &&
  stamp?.target === target &&
  stamp?.fingerprint === currentFingerprint;

if (canSkipBuild) {
  console.log(`pm-cli 未变化，跳过 release 编译：src-tauri/binaries/${sidecarName}`);
} else {
  console.log("pm-cli 源码或构建环境已变化，执行 cargo build --release --bin pm-cli……");
  execFileSync(
    "cargo",
    ["build", "--release", "--bin", "pm-cli", "--manifest-path", manifestArg],
    { cwd: root, stdio: "inherit" },
  );

  execFileSync(process.execPath, [path.join(root, "scripts", "copy-cli.mjs")], {
    cwd: root,
    stdio: "inherit",
  });

  const finalFiles = inputFiles();
  writeFileSync(
    stampPath,
    `${JSON.stringify(
      {
        schemaVersion: 1,
        target,
        fingerprint: fingerprint(finalFiles, compilerInfo, target),
        rustc: compilerInfo,
        builtAt: new Date().toISOString(),
      },
      null,
      2,
    )}\n`,
    "utf8",
  );
}

execFileSync(
  process.execPath,
  [path.join(root, "scripts", "package-cli-skill.mjs"), sidecarPath],
  { cwd: root, stdio: "inherit" },
);
