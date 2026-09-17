import hashlib
import os
import shutil
import subprocess

PROJECT_DIR = os.path.dirname(os.path.abspath(__file__))
DEPS_STAMP = os.path.join(PROJECT_DIR, "node_modules", ".pm-deps-stamp")


def deps_fingerprint():
    """依赖清单的内容指纹（package.json + package-lock.json）。

    用内容而不是修改时间：换机器、git 检出、CI 缓存都会打乱 mtime。
    """
    digest = hashlib.sha256()
    for name in ("package.json", "package-lock.json"):
        path = os.path.join(PROJECT_DIR, name)
        digest.update(name.encode("utf-8"))
        if os.path.exists(path):
            with open(path, "rb") as handle:
                digest.update(handle.read())
        else:
            digest.update(b"-")
    return digest.hexdigest()


def check_python():
    if shutil.which("python") is None and shutil.which("python3") is None:
        print("未找到 Python，请安装 Python 3 并加入 PATH。")
        input("\n按回车键退出……")
        return False
    return True


def check_node():
    node = shutil.which("node")
    if node is None:
        print("未找到 Node.js，请安装 Node.js 20+ 并加入 PATH。")
        input("\n按回车键退出……")
        return False
    ret = subprocess.run(["node", "-v"], capture_output=True, text=True)
    version = ret.stdout.strip().lstrip("v")
    major = int(version.split(".")[0])
    if major < 20:
        print(f"检测到 Node.js {version}，需要 20+。")
        input("\n按回车键退出……")
        return False
    print(f"Node.js {version}：正常")
    return True


def check_npm():
    if shutil.which("npm") is None:
        print("未找到 npm。")
        input("\n按回车键退出……")
        return False
    return True


def check_rust():
    rustc = shutil.which("rustc")
    cargo = shutil.which("cargo")
    if rustc is None or cargo is None:
        print("未找到 Rust 工具链，请先从 https://rustup.rs 安装。")
        input("\n按回车键退出……")
        return False
    ret = subprocess.run(["rustc", "-V"], capture_output=True, text=True)
    print(f"{ret.stdout.strip()}：正常")
    return True


def install_deps():
    package_json = os.path.join(PROJECT_DIR, "package.json")
    node_modules = os.path.join(PROJECT_DIR, "node_modules")
    if not os.path.exists(package_json):
        print("未找到 package.json。")
        input("\n按回车键退出……")
        return False
    fingerprint = deps_fingerprint()
    # 只看 node_modules 目录在不在，会漏掉「清单新增了包、旧目录里没有」的情况：
    # 加 @types/node 那一次就是这样 —— 检查通过、跳过安装，vue-tsc 才抛出一堆
    # 找不到 node: 模块的错。所以按依赖清单的指纹判断是否需要重装。
    if os.path.exists(node_modules) and os.path.exists(DEPS_STAMP):
        try:
            with open(DEPS_STAMP, "r", encoding="utf-8") as handle:
                if handle.read().strip() == fingerprint:
                    print("npm 依赖已安装。")
                    return True
        except OSError:
            pass
    if os.path.exists(node_modules):
        print("npm 依赖与依赖清单不一致，正在重新安装……")
    else:
        print("未找到 node_modules，正在安装 npm 依赖……")
    ret = subprocess.run(["npm", "ci"], cwd=PROJECT_DIR, shell=True)
    if ret.returncode != 0:
        # lock 与 package.json 不同步时 npm ci 会直接失败，退一步用 npm install 兜底。
        print("npm ci 失败，改用 npm install 重试……")
        ret = subprocess.run(["npm", "install"], cwd=PROJECT_DIR, shell=True)
        if ret.returncode != 0:
            print("npm 依赖安装失败。")
            input("\n按回车键退出……")
            return False
    try:
        with open(DEPS_STAMP, "w", encoding="utf-8") as handle:
            handle.write(fingerprint + "\n")
    except OSError:
        # 标记写不进去只是下次会重装一遍，不影响本次启动。
        pass
    print("npm 依赖安装完成。")
    return True


def build_frontend():
    # 浏览器里的任务表格页由 axum 从 dist/ 提供（rust-embed），
    # 而 tauri dev 的 beforeDevCommand 只用 vite 服务设置窗，不会重建 dist。
    # 这里先补一次前端构建；build-frontend.mjs 有指纹缓存，源码没变时直接跳过。
    print("正在构建前端 dist/（源码未变化时自动跳过）……")
    ret = subprocess.run(["npm", "run", "build"], cwd=PROJECT_DIR, shell=True)
    if ret.returncode != 0:
        print("前端构建失败。")
        input("\n按回车键退出……")
        return False
    return True


def run_dev():
    print("启动 Tauri 开发模式（设置窗 + 内嵌 HTTP 服务）……")
    print("提示：dev 模式下数据目录为项目根 data/；浏览器访问 http://127.0.0.1:<端口>/ 看表格。")
    subprocess.run(["npm", "run", "tauri", "dev"], cwd=PROJECT_DIR, shell=True)


def main():
    all_ok = True
    print("正在检查开发环境……")
    all_ok = all_ok and check_python()
    all_ok = all_ok and check_npm()
    all_ok = all_ok and check_node()
    all_ok = all_ok and check_rust()
    print()

    if not all_ok:
        input("\n按回车键退出……")
        return

    if not install_deps():
        input("\n按回车键退出……")
        return

    if not build_frontend():
        return

    run_dev()
    print("\n开发模式已退出。")
    input("\n按回车键退出……")


if __name__ == "__main__":
    main()
