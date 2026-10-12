//! Windows capture exclusion for undetectable mode.
//!
//! Applies `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)` to every
//! top-level window of this process. This is the same primitive Electron's
//! `setContentProtection` uses on Windows and the same one the Natively
//! reference re-asserts every 500 ms while undetectable, because the OS can
//! silently drop the affinity (focus changes, DWM churn).
//!
//! The sweep also covers windows the shell does not own directly — Tauri
//! dialogs, WebView2 popups — exactly like the reference's
//! `capture_exclusion.rs`, but in-process without a native Node module.

#![cfg(windows)]
#![allow(clippy::missing_errors_doc)]

use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowDisplayAffinity, GetWindowThreadProcessId, SetWindowDisplayAffinity,
    WDA_EXCLUDEFROMCAPTURE, WDA_MONITOR, WDA_NONE,
};

/// Exclude (or restore) every top-level window of this process in captures.
///
/// Returns how many windows were changed. Idempotent: windows already in the
/// requested state are skipped, so the re-assert loop is cheap.
pub fn set_capture_excluded(excluded: bool) -> u32 {
    let mut sweep = Sweep {
        pid: unsafe { GetCurrentProcessId() },
        excluded,
        changed: 0,
    };
    // SAFETY: `sweep` outlives the synchronous EnumWindows call and the
    // callback only reaches it through the LPARAM it was given.
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(&mut sweep as *mut Sweep as isize));
    }
    sweep.changed
}

struct Sweep {
    pid: u32,
    excluded: bool,
    changed: u32,
}

unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let sweep = &mut *(lparam.0 as *mut Sweep);
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid != sweep.pid {
        return BOOL(1);
    }
    let mut current = 0u32;
    let _ = unsafe { GetWindowDisplayAffinity(hwnd, &mut current) };
    if sweep.excluded {
        if current == WDA_NONE.0 {
            // Before Windows 10 2004 the exclusion shows as a black box
            // (WDA_MONITOR semantics); try the strict flag first, then the
            // compatible one so older builds still get protection.
            if unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) }.is_ok()
                || unsafe { SetWindowDisplayAffinity(hwnd, WDA_MONITOR) }.is_ok()
            {
                sweep.changed += 1;
            }
        }
    } else if current != WDA_NONE.0 && unsafe { SetWindowDisplayAffinity(hwnd, WDA_NONE) }.is_ok() {
        sweep.changed += 1;
    }
    BOOL(1)
}

/// Read the current capture-exclusion state of one window (diagnostics).
pub fn window_excluded(window: &tauri::WebviewWindow) -> bool {
    let Ok(hwnd) = window.hwnd() else {
        return false;
    };
    let mut current = 0u32;
    if unsafe { GetWindowDisplayAffinity(hwnd, &mut current) }.is_err() {
        return false;
    }
    current != WDA_NONE.0
}
