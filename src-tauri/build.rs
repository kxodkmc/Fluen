fn main() {
    // 注入 target-triple：运行期解析 Tauri externalBin sidecar 文件名用
    println!(
        "cargo:rustc-env=FLUEN_TARGET_TRIPLE={}",
        std::env::var("TARGET").unwrap_or_default()
    );
    tauri_build::build()
}
