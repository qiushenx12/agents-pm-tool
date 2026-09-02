// 把编译好的 pm-cli 复制为 Tauri externalBin 约定的命名（<name>-<target-triple>），
// 打包时会以 pm-cli.exe 安装到主 exe 同目录（规划 §1.1）。
import { copyFileSync, mkdirSync } from "node:fs";
import { execSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

// 从 rustc 取 host triple，避免硬编码
const triple = execSync("rustc -vV", { encoding: "utf8" })
  .match(/host:\s*(\S+)/)[1];

const isWindows = process.platform === "win32";
const src = path.join(root, "src-tauri", "target", "release", isWindows ? "pm-cli.exe" : "pm-cli");
const destDir = path.join(root, "src-tauri", "binaries");
const dest = path.join(destDir, `pm-cli-${triple}${isWindows ? ".exe" : ""}`);

mkdirSync(destDir, { recursive: true });
copyFileSync(src, dest);
console.log(`pm-cli → ${path.relative(root, dest)}`);
