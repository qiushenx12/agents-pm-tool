import os
import shutil
import subprocess

PROJECT_DIR = os.path.dirname(os.path.abspath(__file__))


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
    if not os.path.exists(node_modules):
        print("未找到 node_modules，正在安装 npm 依赖……")
        ret = subprocess.run(["npm", "install"], cwd=PROJECT_DIR, shell=True)
        if ret.returncode != 0:
            print("npm install 失败。")
            input("\n按回车键退出……")
            return False
        print("npm 依赖安装完成。")
    else:
        print("npm 依赖已安装。")
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

    run_dev()
    print("\n开发模式已退出。")
    input("\n按回车键退出……")


if __name__ == "__main__":
    main()
