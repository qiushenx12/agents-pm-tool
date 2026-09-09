"""Agents PM Tool 正式打包脚本（Windows / NSIS）。

功能与 cc-launcher/build.py 对齐：
- version.json 驱动的版本管理（发布后自动递增 patch，0.0.9 → 0.1.0）
- 版本号同步 package.json / package-lock.json / tauri.conf.json / Cargo.toml / Cargo.lock
- npm run tauri build（beforeBuildCommand = build:all，会连带编译前端 + pm-cli）
- 产物归档到 src-tauri/release-bundle/nsis/，bundle 目录里保留历史安装包
- 打包完成后交互确认测试是否通过，通过则记录为已发布

注意：Cargo.lock 在 .gitignore 里（binary 项目惯例可提交，本项目选择忽略），
脚本会在打包后自动生成/同步它，无需纳入版本管理。
"""

from __future__ import annotations

import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
from datetime import datetime
from pathlib import Path
from time import perf_counter
from typing import Any


PROJECT_DIR = Path(__file__).resolve().parent
VERSION_FILE = PROJECT_DIR / "version.json"
DEFAULT_VERSION = "1.0.0"
VERSION_PATTERN = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")
PACKAGE_NAME = "agents-pm-tool"  # Cargo.toml [package].name
PLATFORM_KEY = "windows"  # 本项目只打 Windows
PLATFORM_LABEL = "Windows"


class VersionStateError(ValueError):
    pass


# ── 文件读写 ─────────────────────────────────────────────────


def atomic_write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temp_path: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w",
            encoding="utf-8",
            newline="",
            dir=path.parent,
            prefix=f".{path.name}.",
            suffix=".tmp",
            delete=False,
        ) as temp_file:
            temp_file.write(content)
            temp_path = Path(temp_file.name)
        os.replace(temp_path, path)
    finally:
        if temp_path is not None and temp_path.exists():
            temp_path.unlink()


def read_json(path: Path) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as error:
        raise VersionStateError(f"文件不存在：{path}") from error
    except json.JSONDecodeError as error:
        raise VersionStateError(f"JSON 格式无效：{path}（{error}）") from error
    if not isinstance(data, dict):
        raise VersionStateError(f"JSON 根节点必须是对象：{path}")
    return data


def write_json(path: Path, data: dict[str, Any]) -> None:
    atomic_write_text(path, json.dumps(data, ensure_ascii=False, indent=2) + "\n")


# ── 版本号 ───────────────────────────────────────────────────


def parse_version(version: str) -> tuple[int, int, int]:
    match = VERSION_PATTERN.fullmatch(version)
    if match is None:
        raise VersionStateError(f"版本号必须使用 major.minor.patch 格式：{version!r}")
    major, minor, patch = (int(part) for part in match.groups())
    if patch > 9:
        raise VersionStateError(f"修订版本号只能是 0 到 9：{version!r}")
    return major, minor, patch


def next_version(version: str) -> str:
    major, minor, patch = parse_version(version)
    if patch < 9:
        return f"{major}.{minor}.{patch + 1}"
    return f"{major}.{minor + 1}.0"


# ── version.json 状态 ────────────────────────────────────────


def default_version_state() -> dict[str, Any]:
    return {
        "schemaVersion": 1,
        "currentVersion": DEFAULT_VERSION,
        "published": False,
        "releases": [],
    }


def validate_version_state(state: dict[str, Any]) -> None:
    if state.get("schemaVersion") != 1:
        raise VersionStateError("version.json 的 schemaVersion 必须为 1")
    current_version = state.get("currentVersion")
    if not isinstance(current_version, str):
        raise VersionStateError("version.json 缺少 currentVersion")
    parse_version(current_version)
    if not isinstance(state.get("published"), bool):
        raise VersionStateError("version.json 的 published 必须是布尔值")
    releases = state.get("releases")
    if not isinstance(releases, list):
        raise VersionStateError("version.json 的 releases 必须是数组")
    seen: set[str] = set()
    for release in releases:
        if not isinstance(release, dict) or not isinstance(release.get("version"), str):
            raise VersionStateError("version.json 中存在无效的发布记录")
        parse_version(release["version"])
        if release["version"] in seen:
            raise VersionStateError(f"version.json 中存在重复发布版本：{release['version']}")
        seen.add(release["version"])


def load_version_state() -> dict[str, Any]:
    if not VERSION_FILE.exists():
        state = default_version_state()
        write_json(VERSION_FILE, state)
        return state
    state = read_json(VERSION_FILE)
    validate_version_state(state)
    return state


def save_version_state(state: dict[str, Any]) -> None:
    validate_version_state(state)
    write_json(VERSION_FILE, state)


# ── 版本号同步到各配置 ───────────────────────────────────────


def replace_cargo_package_version(path: Path, version: str) -> None:
    content = path.read_text(encoding="utf-8")
    if path.name == "Cargo.toml":
        package_start = content.find("[package]")
        if package_start < 0:
            raise VersionStateError(f"未在 {path} 中找到 [package]")
        next_section = content.find("\n[", package_start + len("[package]"))
        if next_section < 0:
            next_section = len(content)
        package_section = content[package_start:next_section]
        updated_section, count = re.subn(
            r'(?m)^version\s*=\s*"[^"]+"',
            f'version = "{version}"',
            package_section,
            count=1,
        )
        if count != 1:
            raise VersionStateError(f"未在 {path} 的 [package] 中找到版本号")
        updated = content[:package_start] + updated_section + content[next_section:]
    else:  # Cargo.lock
        pattern = re.compile(
            rf'(\[\[package\]\]\s+name\s*=\s*"{re.escape(PACKAGE_NAME)}"\s+'
            r'version\s*=\s*")[^"]+("\s*)',
        )
        updated, count = pattern.subn(rf"\g<1>{version}\g<2>", content, count=1)
        if count != 1:
            # Cargo.lock 可能还没生成，不视为错误
            return
    if updated != content:
        atomic_write_text(path, updated)


def write_json_if_changed(path: Path, data: dict[str, Any], original: dict[str, Any]) -> None:
    """版本号没变就不重写，避免 JSON 重排造成的格式噪音。"""
    if data == original:
        return
    write_json(path, data)


def sync_project_versions(version: str) -> None:
    parse_version(version)

    package_path = PROJECT_DIR / "package.json"
    package_data = read_json(package_path)
    if package_data.get("version") != version:
        original = dict(package_data)
        package_data["version"] = version
        write_json_if_changed(package_path, package_data, original)

    package_lock_path = PROJECT_DIR / "package-lock.json"
    if package_lock_path.exists():
        package_lock_data = read_json(package_lock_path)
        root_package = package_lock_data.get("packages", {}).get("")
        needs_write = package_lock_data.get("version") != version or (
            isinstance(root_package, dict) and root_package.get("version") != version
        )
        if needs_write:
            package_lock_data["version"] = version
            if isinstance(root_package, dict):
                root_package["version"] = version
            write_json(package_lock_path, package_lock_data)

    tauri_config_path = PROJECT_DIR / "src-tauri" / "tauri.conf.json"
    tauri_config = read_json(tauri_config_path)
    if tauri_config.get("version") != version:
        original = dict(tauri_config)
        tauri_config["version"] = version
        write_json_if_changed(tauri_config_path, tauri_config, original)

    replace_cargo_package_version(PROJECT_DIR / "src-tauri" / "Cargo.toml", version)
    cargo_lock = PROJECT_DIR / "src-tauri" / "Cargo.lock"
    if cargo_lock.exists():
        replace_cargo_package_version(cargo_lock, version)


def prepare_build_version() -> tuple[str, dict[str, Any]]:
    state = load_version_state()
    version = state["currentVersion"]
    if state["published"]:
        version = next_version(version)
        state["currentVersion"] = version
        state["published"] = False
        print(f"上一个版本已发布，本次打包版本自动更新为 {version}。")
    sync_project_versions(version)
    save_version_state(state)
    return version, state


# ── 环境检查 ─────────────────────────────────────────────────


def check_rust() -> bool:
    rustc = shutil.which("rustc")
    cargo = shutil.which("cargo")
    if rustc is None or cargo is None:
        print("未找到 Rust 工具链，请先从 https://rustup.rs 安装。")
        return False
    result = subprocess.run([rustc, "-V"], capture_output=True, text=True)
    if result.returncode != 0:
        print("Rust 工具链检查失败。")
        return False
    print(f"{result.stdout.strip()}：正常")
    return True


def find_npm() -> str | None:
    return shutil.which("npm.cmd") or shutil.which("npm")


def check_npm() -> bool:
    npm = find_npm()
    if npm is None:
        print("未找到 npm，请先安装 Node.js 并添加到 PATH。")
        return False
    result = subprocess.run([npm, "--version"], capture_output=True, text=True)
    if result.returncode != 0:
        print("npm 检查失败。")
        return False
    print(f"npm {result.stdout.strip()}：正常")
    return True


def install_deps() -> bool:
    package_json = PROJECT_DIR / "package.json"
    node_modules = PROJECT_DIR / "node_modules"
    if not package_json.exists():
        print("未找到 package.json。")
        return False
    if node_modules.exists():
        print("npm 依赖已安装。")
        return True
    npm = find_npm()
    if npm is None:
        return False
    print("未找到 node_modules，正在安装 npm 依赖……")
    result = subprocess.run([npm, "install"], cwd=PROJECT_DIR)
    if result.returncode != 0:
        print("npm install 失败。")
        return False
    print("npm 依赖安装完成。")
    return True


# ── 打包产物管理 ─────────────────────────────────────────────


def load_product_name() -> str:
    config = read_json(PROJECT_DIR / "src-tauri" / "tauri.conf.json")
    product_name = config.get("productName")
    if not isinstance(product_name, str) or not product_name.strip():
        raise VersionStateError("tauri.conf.json 缺少 productName")
    return product_name


def bundle_dir() -> Path:
    return PROJECT_DIR / "src-tauri" / "target" / "release" / "bundle" / "nsis"


def archive_dir() -> Path:
    return PROJECT_DIR / "src-tauri" / "release-bundle" / "nsis"


def is_current_artifact(filename: str, product_name: str, version: str) -> bool:
    return filename.startswith(f"{product_name}_{version}_") and filename.endswith("-setup.exe")


def restore_archived_artifacts() -> int:
    """把归档目录里的历史安装包拷回 bundle 目录（tauri build 会清空 bundle 目录）。"""
    arc = archive_dir()
    if not arc.exists():
        return 0
    dst = bundle_dir()
    dst.mkdir(parents=True, exist_ok=True)
    restored = 0
    for archived in arc.glob("*.exe"):
        target = dst / archived.name
        if not target.exists():
            shutil.copy2(archived, target)
            restored += 1
    return restored


def archive_artifacts(artifacts: list[Path]) -> list[Path]:
    arc = archive_dir()
    arc.mkdir(parents=True, exist_ok=True)
    archived_paths: list[Path] = []
    for artifact in artifacts:
        archived_path = arc / artifact.name
        shutil.copy2(artifact, archived_path)
        archived_paths.append(archived_path)
    return archived_paths


def run_build(version: str, product_name: str) -> bool:
    npm = find_npm()
    if npm is None:
        return False

    bundle_dir().mkdir(parents=True, exist_ok=True)
    archived_names = {
        artifact.name for artifact in archive_dir().glob("*.exe")
    }
    # 已归档的历史包不需要在构建前再次复制到 bundle；构建完成后统一恢复即可。
    # 这里只备份尚未归档的当前版本包，以便构建失败时保留它。
    existing = [
        artifact
        for artifact in bundle_dir().glob("*.exe")
        if artifact.name not in archived_names
    ]
    print(f"正在打包 Agents PM Tool {version}（Windows NSIS）……")

    with tempfile.TemporaryDirectory(prefix="agents-pm-tool-installer-history-") as backup:
        backup_path = Path(backup)
        for artifact in existing:
            shutil.copy2(artifact, backup_path / artifact.name)

        # tauri build 的 beforeBuildCommand 是 build:all（前端 + pm-cli）
        started_at = perf_counter()
        result = subprocess.run([npm, "run", "tauri", "build"], cwd=PROJECT_DIR)
        print(f"Tauri 构建耗时：{perf_counter() - started_at:.2f} 秒。")

        restored = restore_archived_artifacts()
        if restored:
            print(f"已从发布归档恢复 {restored} 个历史安装包。")

        kept = 0
        for artifact in existing:
            if is_current_artifact(artifact.name, product_name, version):
                if result.returncode != 0 and not artifact.exists():
                    shutil.copy2(backup_path / artifact.name, artifact)
                    kept += 1
                continue
            if not artifact.exists():
                shutil.copy2(backup_path / artifact.name, artifact)
                kept += 1

    if kept:
        print(f"已保留 {kept} 个历史安装包。")
    if result.returncode != 0:
        print(f"\nWindows 版本 {version} 打包失败；版本号不会递增。")
        return False
    return True


def find_built_artifacts(version: str, product_name: str) -> list[Path]:
    bd = bundle_dir()
    if not bd.exists():
        return []
    return sorted(
        p for p in bd.glob("*.exe") if is_current_artifact(p.name, product_name, version)
    )


def normalized_architecture() -> str:
    value = platform.machine().lower()
    if value in {"amd64", "x86_64"}:
        return "x64"
    if value in {"arm64", "aarch64"}:
        return "arm64"
    return value or "unknown"


def record_released(version: str, artifacts: list[Path]) -> None:
    state = load_version_state()
    if state["currentVersion"] != version:
        raise VersionStateError(
            f"待记录版本 {version} 与 version.json 中的 {state['currentVersion']} 不一致"
        )
    if state["published"]:
        raise VersionStateError(f"版本 {version} 已经发布")
    if not artifacts:
        raise VersionStateError("没有可记录的安装包")

    archived = archive_artifacts(artifacts)
    artifact_records = []
    for artifact in archived:
        try:
            relative_path = artifact.relative_to(PROJECT_DIR).as_posix()
        except ValueError:
            relative_path = str(artifact)
        artifact_records.append({"path": relative_path, "size": artifact.stat().st_size})

    if any(r.get("version") == version for r in state["releases"]):
        raise VersionStateError(f"版本 {version} 已存在发布记录")
    state["published"] = True
    state["releases"].append(
        {
            "version": version,
            "publishedAt": datetime.now().astimezone().isoformat(timespec="seconds"),
            "architecture": normalized_architecture(),
            "artifacts": artifact_records,
        }
    )
    save_version_state(state)


# ── 入口 ─────────────────────────────────────────────────────


def pause_on_error() -> None:
    input("\n按回车键退出……")


def main() -> int:
    if sys.platform != "win32":
        print(f"当前系统不支持正式打包（仅 Windows）：{sys.platform}")
        pause_on_error()
        return 1

    print(f"正在检查 {PLATFORM_LABEL} 打包环境……")
    if not check_npm() or not check_rust():
        pause_on_error()
        return 1
    if not install_deps():
        pause_on_error()
        return 1

    try:
        version, _state = prepare_build_version()
        product_name = load_product_name()
    except (OSError, VersionStateError) as error:
        print(f"版本准备失败：{error}")
        pause_on_error()
        return 1

    if not run_build(version, product_name):
        pause_on_error()
        return 1

    artifacts = find_built_artifacts(version, product_name)
    if not artifacts:
        print(f"打包命令已结束，但没有找到 {PLATFORM_LABEL} 版本 {version} 的安装包。")
        pause_on_error()
        return 1

    print("\n打包完成：")
    for artifact in artifacts:
        print(f"  {artifact}")

    test_input = (
        input(f"\n{PLATFORM_LABEL} {version} 测试通过请输入 r 后回车；测试未通过请直接回车：")
        .strip()
        .lower()
    )
    if test_input != "r":
        print(f"{PLATFORM_LABEL} {version} 保持待发布；下一次打包仍使用该版本号。")
        return 0

    try:
        record_released(version, artifacts)
    except (OSError, VersionStateError) as error:
        print(f"发布记录写入失败：{error}")
        pause_on_error()
        return 1

    print(f"版本 {version} 已记录为发布；下一次打包将自动使用 {next_version(version)}。")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
