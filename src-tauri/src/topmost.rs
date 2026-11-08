//! Keeps the main window above other windows, including games in borderless or "optimized"
//! fullscreen.
//!
//! Tauri's `set_always_on_top(true)` does nothing when the flag is already set, so it can't push
//! the window back to the top after another topmost window (a game, usually) has covered it. This
//! module calls `SetWindowPos(HWND_TOPMOST)` directly. It does that when another app comes to the
//! foreground (and again shortly after, because games often make themselves topmost a moment
//! after they activate), plus once a second as a fallback.
//!
//! Games in true exclusive fullscreen own the display, so no normal window can draw over them.
//! Borderless/windowed fullscreen, and DirectX games that use Windows' fullscreen optimizations,
//! do allow it.

use crate::state::AppState;
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowPos, EVENT_SYSTEM_FOREGROUND, HWND_TOPMOST, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOOWNERZORDER, SWP_NOSIZE, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};

static APP: OnceLock<AppHandle> = OnceLock::new();

fn main_hwnd(app: &AppHandle) -> Option<HWND> {
    let w = crate::window::main_window(app)?;
    let raw = w.hwnd().ok()?;
    Some(HWND(raw.0 as _))
}

fn wanted(app: &AppHandle) -> bool {
    let st = app.state::<Arc<AppState>>();
    st.settings().always_on_top && st.window_visible.load(Ordering::Relaxed)
}

/// Puts the main window on top of the topmost band without activating it.
pub fn raise(app: &AppHandle) {
    let Some(hwnd) = main_hwnd(app) else { return };
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_ASYNCWINDOWPOS,
        );
    }
}

fn raise_if_wanted(app: &AppHandle) {
    if wanted(app) {
        raise(app);
    }
}

unsafe extern "system" fn on_foreground(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    let Some(app) = APP.get() else { return };
    if !wanted(app) {
        return;
    }
    raise(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        for ms in [150u64, 600, 1500] {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            raise_if_wanted(&app);
        }
    });
}

/// Must run on the main thread (Tauri `setup`): the hook delivers its events through that
/// thread's message loop.
pub fn install(app: &AppHandle) {
    let _ = APP.set(app.clone());
    let hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(on_foreground),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };
    if hook.is_invalid() {
        log::warn!("topmost: foreground hook unavailable");
    }

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            raise_if_wanted(&app);
        }
    });
}
