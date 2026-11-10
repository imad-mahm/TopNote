//! Keeps the main window above other windows on X11 (also GNOME/KDE on Wayland, where TopNote
//! runs through XWayland; see `lib.rs`).
//!
//! `set_always_on_top(true)` sets `_NET_WM_STATE_ABOVE` once. Window managers keep that window in
//! the "above" layer, but a game that maps itself afterwards (or goes borderless-fullscreen) can
//! still end up stacked over it. So this module asks the window manager to restack TopNote to the
//! top of its layer whenever another app becomes the active window (and again shortly after,
//! because games often reorder themselves a moment after they activate), plus once a second as a
//! fallback.
//!
//! Limits: a window that is in the real `_NET_WM_STATE_FULLSCREEN` state is in a layer above
//! "always on top" in some window managers (GNOME's Mutter, for instance), and no client can draw
//! over it there. Borderless/windowed-fullscreen games, which is what most games use, are fine.

use crate::state::AppState;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tauri::{AppHandle, Manager};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ChangeWindowAttributesAux, ClientMessageEvent, ConnectionExt, EventMask};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

fn xid(app: &AppHandle) -> Option<u32> {
    let w = crate::window::main_window(app)?;
    match w.window_handle().ok()?.as_raw() {
        RawWindowHandle::Xlib(h) => Some(h.window as u32),
        RawWindowHandle::Xcb(h) => Some(h.window.get()),
        _ => None, // native Wayland: clients can't position themselves in the stack
    }
}

fn wanted(app: &AppHandle) -> bool {
    let st = app.state::<Arc<AppState>>();
    st.settings().always_on_top && st.window_visible.load(Ordering::Relaxed)
}

fn atom(c: &RustConnection, name: &str) -> Option<u32> {
    c.intern_atom(false, name.as_bytes()).ok()?.reply().ok().map(|r| r.atom)
}

fn send_to_root(c: &RustConnection, root: u32, window: u32, msg_type: u32, data: [u32; 5]) {
    let ev = ClientMessageEvent::new(32, window, msg_type, data);
    let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
    let _ = c.send_event(false, root, mask, ev);
}

/// Puts the main window on top of the "above" layer without activating it.
pub fn raise(app: &AppHandle) {
    let Some(win) = xid(app) else { return };
    let Ok((c, screen)) = x11rb::connect(None) else { return };
    let root = c.setup().roots[screen].root;
    let (Some(state), Some(above), Some(restack)) = (
        atom(&c, "_NET_WM_STATE"),
        atom(&c, "_NET_WM_STATE_ABOVE"),
        atom(&c, "_NET_RESTACK_WINDOW"),
    ) else {
        return;
    };
    // make sure the flag is there (1 = add) …
    send_to_root(&c, root, win, state, [1, above, 0, 1, 0]);
    // … then move to the top (source indication 2 = pager/taskbar-like tool, sibling none, detail Above)
    send_to_root(&c, root, win, restack, [2, 0, 0, 0, 0]);
    let _ = c.flush();
}

fn raise_if_wanted(app: &AppHandle) {
    if wanted(app) {
        raise(app);
    }
}

/// Watches `_NET_ACTIVE_WINDOW` on the root window from its own thread.
fn watch_foreground(app: AppHandle) {
    std::thread::Builder::new()
        .name("topmost".into())
        .spawn(move || {
            let Ok((c, screen)) = x11rb::connect(None) else {
                log::warn!("topmost: no X11 connection, foreground watcher disabled");
                return;
            };
            let root = c.setup().roots[screen].root;
            let Some(active) = atom(&c, "_NET_ACTIVE_WINDOW") else { return };
            if c.change_window_attributes(root, &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE))
                .is_err()
            {
                return;
            }
            let _ = c.flush();
            while let Ok(ev) = c.wait_for_event() {
                if let Event::PropertyNotify(p) = ev {
                    if p.atom == active && wanted(&app) {
                        raise(&app);
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            for ms in [150u64, 600, 1500] {
                                tokio::time::sleep(Duration::from_millis(ms)).await;
                                raise_if_wanted(&app);
                            }
                        });
                    }
                }
            }
        })
        .ok();
}

/// Call from Tauri `setup`, after the main window exists.
pub fn install(app: &AppHandle) {
    watch_foreground(app.clone());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            raise_if_wanted(&app);
        }
    });
}
