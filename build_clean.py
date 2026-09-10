"""双击运行：清理构建产物，保留最终安装包。

清理：
- src-tauri/target/ —— Rust 编译缓存（约 1.7G），保留 release/bundle/ 里的安装包
- dist/ —— 前端构建产物，npm run build 可重新生成

保留：
- node_modules/（依赖，非构建产物）
- src-tauri/binaries/（pm-cli sidecar，打包必需）
- src-tauri/release-bundle/（历史安装包归档）
"""

from __future__ import annotations

import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
TARGET = ROOT / "src-tauri" / "target"
DIST = ROOT / "dist"

# target 内要保留的路径（相对 target），其余删除
KEEP = ("release", "bundle")


def dir_size(path: Path) -> int:
    total = 0
    for p in path.rglob("*"):
        if p.is_file() and not p.is_symlink():
            try:
                total += p.stat().st_size
            except OSError:
                pass
    return total


def remove(path: Path) -> None:
    if path.is_dir():
        shutil.rmtree(path, ignore_errors=True)
    else:
        path.unlink(missing_ok=True)


def clean_target() -> int:
    """清空 target，但保留 release/bundle（最终安装包）。"""
    if not TARGET.exists():
        return 0
    keep_dir = TARGET.joinpath(*KEEP)
    # 保留目录不存在时整个 target 都可删
    if not keep_dir.exists():
        size = dir_size(TARGET)
        remove(TARGET)
        return size
    freed = 0
    for child in TARGET.iterdir():
        if child.name != KEEP[0]:
            freed += dir_size(child) if child.is_dir() else child.stat().st_size
            remove(child)
    release = TARGET / KEEP[0]
    for child in release.iterdir():
        if child.name != KEEP[1]:
            freed += dir_size(child) if child.is_dir() else child.stat().st_size
            remove(child)
    return freed


def main() -> None:
    freed = clean_target()
    if DIST.exists():
        freed += dir_size(DIST)
        remove(DIST)
    print(f"清理完成，释放约 {freed / 1024 / 1024:.0f} MB")
    bundle = TARGET.joinpath(*KEEP)
    if bundle.exists():
        print(f"最终安装包已保留：{bundle}")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:  # noqa: BLE001 —— 双击运行时把错误留在窗口里
        print(f"清理失败：{exc}")
    if sys.stdin.isatty():
        input("按回车关闭窗口…")
