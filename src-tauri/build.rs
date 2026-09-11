fn main() {
    // 测试二进制默认没有 Common-Controls v6 的清单依赖，comctl32 会按旧版 5.82 加载，
    // 而托盘/窗口代码用到的 SetWindowSubclass 等只存在于 v6 —— 表现为测试进程
    // 启动即 0xc0000139（STATUS_ENTRYPOINT_NOT_FOUND）。这里给测试目标补上清单依赖。
    #[cfg(windows)]
    println!(
        "cargo:rustc-link-arg-tests=/MANIFESTDEPENDENCY:type='win32' \
         name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
         processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
    );
    tauri_build::build()
}
