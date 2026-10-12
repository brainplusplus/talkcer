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
    EnumWindows, GetWindowDisplayAffinity, GetWindowLongPtrW, GetWindowThreadProcessId,
    SetWindowDisplayAffinity, SetWindowLongPtrW, GWL_EXSTYLE, WDA_EXCLUDEFROMCAPTURE, WDA_MONITOR,
    WDA_NONE, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
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

/// Count this process's top-level windows, split by capture exclusion.
///
/// Diagnostics only. A live check reads this to confirm the affinity stuck,
/// which a window title or taskbar button cannot show.
pub fn exclusion_counts() -> (u32, u32) {
    let mut tally = Tally {
        pid: unsafe { GetCurrentProcessId() },
        excluded: 0,
        plain: 0,
    };
    // SAFETY: `tally` outlives the synchronous EnumWindows call.
    unsafe {
        let _ = EnumWindows(Some(count_visit), LPARAM(&mut tally as *mut Tally as isize));
    }
    (tally.excluded, tally.plain)
}

struct Tally {
    pid: u32,
    excluded: u32,
    plain: u32,
}

unsafe extern "system" fn count_visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let tally = &mut *(lparam.0 as *mut Tally);
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid != tally.pid {
        return BOOL(1);
    }
    let mut current = 0u32;
    if unsafe { GetWindowDisplayAffinity(hwnd, &mut current) }.is_ok() {
        if current == WDA_NONE.0 {
            tally.plain += 1;
        } else {
            tally.excluded += 1;
        }
    }
    BOOL(1)
}

/// Hide or show this process's top-level windows on the taskbar.
///
/// `ITaskbarList::DeleteTab` (what `set_skip_taskbar` calls) only lasts until the
/// window is activated, so it never sticks. `WS_EX_TOOLWINDOW` keeps it off the
/// taskbar without covering it with an owned window.
pub fn set_taskbar_visible(visible: bool) -> u32 {
    let mut sweep = TaskbarSweep {
        pid: unsafe { GetCurrentProcessId() },
        visible,
        changed: 0,
    };
    // SAFETY: `sweep` outlives the synchronous EnumWindows call.
    unsafe {
        let _ = EnumWindows(Some(taskbar_visit), LPARAM(&mut sweep as *mut TaskbarSweep as isize));
    }
    sweep.changed
}

struct TaskbarSweep {
    pid: u32,
    visible: bool,
    changed: u32,
}

unsafe extern "system" fn taskbar_visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let sweep = &mut *(lparam.0 as *mut TaskbarSweep);
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid != sweep.pid {
        return BOOL(1);
    }
    let before = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    let after = if sweep.visible {
        (before & !WS_EX_TOOLWINDOW.0) | WS_EX_APPWINDOW.0
    } else {
        (before & !WS_EX_APPWINDOW.0) | WS_EX_TOOLWINDOW.0
    };
    if after != before {
        unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, after as isize) };
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
