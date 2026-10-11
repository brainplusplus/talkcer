//! Desktop entry point.
//!
//! This build plans the overlay window. It does not open a native window
//! because the Tauri CLI is not part of the default toolchain.

fn main() {
    println!("Talkcer desktop shell");
    println!("API: {}", talkcer_desktop::DEFAULT_API);
    println!("Overlay shortcut: {}", talkcer_desktop::overlay::OVERLAY_SHORTCUT);
    println!("The native window is not bundled. Use the /overlay page and the headless window plan.");
}
