use crate::state::AppState;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Wry};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

const SNAP_PX: i32 = 16;

pub fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("main")
}

pub struct TrayState {
    pub click_through: CheckMenuItem<Wry>,
}

pub fn toggle_visible(app: &AppHandle) {
    let Some(w) = main_window(app) else { return };
    let st = app.state::<Arc<AppState>>();
    if w.is_visible().unwrap_or(true) {
        let _ = w.hide();
        st.window_visible.store(false, Ordering::Relaxed);
    } else {
        let _ = w.show();
        let _ = w.set_focus();
        st.window_visible.store(true, Ordering::Relaxed);
        st.poll_now.notify_one();
    }
}

pub fn show(app: &AppHandle) {
    if let Some(w) = main_window(app) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        app.state::<Arc<AppState>>().window_visible.store(true, Ordering::Relaxed);
    }
}

pub fn set_click_through(app: &AppHandle, on: bool) {
    if let Some(w) = main_window(app) {
        let _ = w.set_ignore_cursor_events(on);
    }
    if let Some(t) = app.try_state::<TrayState>() {
        let _ = t.click_through.set_checked(on);
    }
    let _ = app.emit("window://clickThrough", on);
}

fn click_through_on(app: &AppHandle) -> bool {
    app.try_state::<TrayState>().and_then(|t| t.click_through.is_checked().ok()).unwrap_or(false)
}

pub fn open_settings(app: &AppHandle) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.unminimize();
        return w.set_focus();
    }
    let mut b = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html#settings".into()))
        .title("Miniplayer Settings")
        .inner_size(480.0, 620.0)
        .min_inner_size(420.0, 480.0)
        .always_on_top(true)
        .resizable(true);
    if let Some(main) = main_window(app) {
        if let Ok(pos) = main.outer_position() {
            b = b.position(pos.x as f64 + 40.0, pos.y as f64 + 40.0);
        }
    }
    b.build()?;
    Ok(())
}

/// Clamp the restored window into a visible monitor (the saved position may be off-screen).
pub fn ensure_on_screen(w: &WebviewWindow) {
    let (Ok(pos), Ok(size), Ok(monitors)) = (w.outer_position(), w.outer_size(), w.available_monitors()) else { return };
    let visible = monitors.iter().any(|m| {
        let mp = m.position();
        let ms = m.size();
        pos.x + 40 < mp.x + ms.width as i32 && pos.x + size.width as i32 > mp.x + 40 && pos.y >= mp.y - 10 && pos.y + 20 < mp.y + ms.height as i32
    });
    if !visible {
        let _ = w.center();
    }
}

fn snap(w: &WebviewWindow) {
    let (Ok(pos), Ok(size), Ok(Some(mon))) = (w.outer_position(), w.outer_size(), w.current_monitor()) else { return };
    let wa = mon.work_area();
    let (left, top) = (wa.position.x, wa.position.y);
    let (right, bottom) = (left + wa.size.width as i32, top + wa.size.height as i32);
    let (mut x, mut y) = (pos.x, pos.y);
    let (w_, h_) = (size.width as i32, size.height as i32);
    if (x - left).abs() <= SNAP_PX {
        x = left;
    } else if (x + w_ - right).abs() <= SNAP_PX {
        x = right - w_;
    }
    if (y - top).abs() <= SNAP_PX {
        y = top;
    } else if (y + h_ - bottom).abs() <= SNAP_PX {
        y = bottom - h_;
    }
    if (x, y) != (pos.x, pos.y) {
        let _ = w.set_position(PhysicalPosition::new(x, y));
    }
}

pub fn setup_main(app: &AppHandle, st: Arc<AppState>) {
    let Some(w) = main_window(app) else { return };
    ensure_on_screen(&w);
    let s = st.settings();
    let _ = w.set_skip_taskbar(s.skip_taskbar);
    let _ = w.set_always_on_top(true);

    // Debounced edge snapping: runs ~180ms after the window stops moving.
    let gen = Arc::new(AtomicU64::new(0));
    let w2 = w.clone();
    let st2 = st.clone();
    w.on_window_event(move |e| match e {
        tauri::WindowEvent::Moved(_) => {
            if !st2.settings().snap_edges {
                return;
            }
            let my = gen.fetch_add(1, Ordering::SeqCst) + 1;
            let gen = gen.clone();
            let w = w2.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(180)).await;
                if gen.load(Ordering::SeqCst) == my {
                    snap(&w);
                }
            });
        }
        tauri::WindowEvent::Focused(false) => {
            let _ = w2.set_always_on_top(true);
        }
        tauri::WindowEvent::CloseRequested { api, .. } => {
            // Closing the miniplayer hides it to tray instead of quitting.
            api.prevent_close();
            let _ = w2.hide();
            st2.window_visible.store(false, Ordering::Relaxed);
        }
        _ => {}
    });

    // Re-assert topmost periodically so fullscreen/other topmost windows don't bury us.
    let w3 = w.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            if w3.is_visible().unwrap_or(false) {
                let _ = w3.set_always_on_top(true);
            }
        }
    });
}

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "toggle", "Show / Hide", true, None::<&str>)?;
    let view = MenuItem::with_id(app, "view", "Toggle lyrics / artwork", true, None::<&str>)?;
    let ct = CheckMenuItem::with_id(app, "clickthrough", "Click-through", true, false, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &view, &ct, &sep, &settings, &quit])?;
    app.manage(TrayState { click_through: ct.clone() });

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().unwrap())
        .tooltip("Miniplayer")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "toggle" => toggle_visible(app),
            "view" => {
                let _ = app.emit("ui://toggleView", ());
            }
            "clickthrough" => {
                // CheckMenuItem already flipped its own state
                let on = click_through_on(app);
                set_click_through(app, on);
            }
            "settings" => {
                let _ = open_settings(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                toggle_visible(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

pub fn register_shortcuts(app: &AppHandle, st: &AppState) {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let sc = st.settings().shortcuts;
    let pairs = [
        (sc.toggle_window, "toggle"),
        (sc.click_through, "clickthrough"),
        (sc.toggle_view, "view"),
        (sc.offset_up, "offsetUp"),
        (sc.offset_down, "offsetDown"),
    ];
    for (accel, action) in pairs {
        let Ok(shortcut) = accel.parse::<Shortcut>() else {
            log::warn!("invalid shortcut {accel}");
            continue;
        };
        let res = gs.on_shortcut(shortcut, move |app, _s, ev| {
            if ev.state != ShortcutState::Pressed {
                return;
            }
            match action {
                "toggle" => toggle_visible(app),
                "clickthrough" => {
                    let on = !click_through_on(app);
                    set_click_through(app, on);
                }
                "view" => {
                    let _ = app.emit("ui://toggleView", ());
                }
                "offsetUp" | "offsetDown" => {
                    let delta = if action == "offsetUp" { 50 } else { -50 };
                    crate::commands::bump_offset(app, delta);
                }
                _ => {}
            }
        });
        if let Err(e) = res {
            log::warn!("could not register {accel}: {e}");
        }
    }
}
