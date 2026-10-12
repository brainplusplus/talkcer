//! Tauri build script. Only runs with the `tauri-shell` feature.

fn main() {
    #[cfg(feature = "tauri-shell")]
    tauri_build::build();
}
