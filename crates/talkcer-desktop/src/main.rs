//! Desktop entry point.
//!
//! With the `tauri-shell` feature the native shell runs: it opens the web UI in
//! the main window and manages the always-on-top overlay. Without the feature
//! the binary prints the headless plan so the workspace builds without the
//! WebView2 toolchain.

fn main() {
    #[cfg(feature = "tauri-shell")]
    {
        if let Err(error) = talkcer_desktop::tauri_shell::run() {
            eprintln!("desktop shell failed: {error}");
            std::process::exit(1);
        }
    }
    #[cfg(not(feature = "tauri-shell"))]
    {
        println!("Talkcer desktop shell (headless)");
        println!("API: {}", talkcer_desktop::DEFAULT_API);
        println!("Workspace: {}", talkcer_desktop::tray::WORKSPACE_URL);
        println!(
            "Overlay shortcut: {}",
            talkcer_desktop::overlay::OVERLAY_SHORTCUT
        );
        println!("Build with --features tauri-shell to open the native window.");
    }
}
