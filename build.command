#!/bin/bash
# Agents PM Tool 双击打包入口（macOS 自用）。
#
# 与 build.py 的区别：不推进版本号、不写发布记录、不询问发布状态，
# 只按当前机器架构产出 DMG。正式发布（同步版本、归档历史包）仍用 python3 build.py。

cd "$(dirname "$0")" || exit 1

# 双击打开的终端走的是登录 shell，PATH 一般没问题；这里兜底补上常见位置，
# 免得 Homebrew / rustup 装的东西在 Finder 启动的会话里找不到。
export PATH="/opt/homebrew/bin:/usr/local/bin:$HOME/.cargo/bin:$PATH"
if ! command -v cargo >/dev/null 2>&1; then
  # rustup 的工具链目录（rustup 本身没进 PATH 时直接用工具链里的 cargo）
  for tc in "$HOME"/.rustup/toolchains/*/bin; do
    if [ -x "$tc/cargo" ]; then
      export PATH="$tc:$PATH"
      break
    fi
  done
fi

# 双击运行的会话没有 tty 交互保障，出错要停下来让人看到；从命令行跑则直接退出。
fail() {
  echo ""
  echo "✗ $1"
  if [ -t 0 ]; then
    read -n 1 -s -r -p "按任意键关闭窗口……"
    echo ""
  fi
  exit 1
}

echo "正在检查打包环境……"
command -v node >/dev/null 2>&1 || fail "未找到 Node.js，请先安装 Node.js 20+。"
command -v npm  >/dev/null 2>&1 || fail "未找到 npm，请先安装 Node.js。"
command -v cargo >/dev/null 2>&1 || fail "未找到 Rust 工具链，请先从 https://rustup.rs 安装。"
echo "node $(node -v) / $(cargo --version)：正常"

# 依赖指纹检查：与 dev.py / build.py 同一套逻辑，清单没变就不重装。
NEED_INSTALL=$(python3 - <<'EOF'
import hashlib
from pathlib import Path

root = Path(".")
digest = hashlib.sha256()
for name in ("package.json", "package-lock.json"):
    p = root / name
    digest.update(name.encode("utf-8"))
    digest.update(p.read_bytes() if p.exists() else b"-")
stamp = root / "node_modules" / ".pm-deps-stamp"
ok = (
    (root / "node_modules").is_dir()
    and stamp.exists()
    and stamp.read_text(encoding="utf-8").strip() == digest.hexdigest()
)
print("skip" if ok else digest.hexdigest())
EOF
) || fail "依赖指纹计算失败（需要 python3）。"

if [ "$NEED_INSTALL" = "skip" ]; then
  echo "npm 依赖已是最新，跳过安装。"
else
  echo "正在安装 npm 依赖……"
  npm ci || npm install || fail "npm 依赖安装失败。"
  printf '%s\n' "$NEED_INSTALL" > node_modules/.pm-deps-stamp
fi

echo ""
echo "正在打包（前端构建 + Tauri release + DMG）……"
# beforeBuildCommand 会先跑 npm run build；源码没变化时前端构建按指纹自动跳过。
npm run tauri build || fail "打包失败，请把上面的错误信息发给维护者。"

DMG_DIR="src-tauri/target/release/bundle/dmg"
echo ""
echo "✓ 打包完成，安装包在：$DMG_DIR/"
ls -1 "$DMG_DIR"/*.dmg 2>/dev/null | sed 's/^/  /'
echo "（覆盖安装不丢数据；首次打开若提示未验证，到 系统设置 → 隐私与安全性 放行一次）"

# 双击运行时顺手打开产物目录；从命令行跑则不打扰。
if [ -t 0 ]; then
  open "$DMG_DIR"
  read -n 1 -s -r -p "按任意键关闭窗口……"
  echo ""
fi
