mod auth;
mod commands;
mod error;
mod logfile;
mod lyrics;
mod playback;
mod secrets;
mod settings;
mod state;
mod stats;
#[cfg(windows)]
mod media_session;
#[cfg(windows)]
mod taskbar;
#[cfg(windows)]
mod topmost;
#[cfg(target_os = "linux")]
#[path = "topmost_x11.rs"]
mod topmost;
mod translate;
mod window;

use state::{AppState, USER_AGENT};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::Manager;

/// One-time move of settings, lyrics cache and stats from the pre-rename config folder.
/// Plugins may create the new folder before setup runs, so key off settings.json, not the folder.
fn migrate_legacy_dir(new_dir: &std::path::Path) {
    let Some(old) = new_dir.parent().map(|p| p.join("lb.imad.spotify-miniplayer")) else { return };
    if new_dir.join("settings.json").exists() || !old.join("settings.json").exists() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(&old) else { return };
    for e in entries.flatten() {
        let src = e.path();
        if src.is_file() {
            let _ = std::fs::copy(&src, new_dir.join(e.file_name()));
        }
    }
    let _ = std::fs::rename(&old, old.with_file_name("lb.imad.spotify-miniplayer.migrated"));
}

/// On a Wayland session, run through XWayland. Wayland gives clients no say over their own stacking
/// order, position or global shortcuts, which is most of what TopNote does: always-on-top, snapping to
/// screen edges, remembered position and the keyboard shortcuts all work on X11 windows.
#[cfg(target_os = "linux")]
fn prefer_x11() {
    let on_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    if on_wayland && std::env::var_os("DISPLAY").is_some() && std::env::var_os("GDK_BACKEND").is_none() {
        std::env::set_var("GDK_BACKEND", "x11");
    }
}

fn window_state_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri_plugin_window_state::{Builder, StateFlags};
    let b = Builder::default().with_state_flags(StateFlags::SIZE | StateFlags::POSITION).with_denylist(&["settings"]);
    // Linux: restored by `window::linux_restore_*` instead. Resizing the window before WebKitGTK has painted
    // for the first time leaves the page blank, which is when the plugin would otherwise do it.
    #[cfg(target_os = "linux")]
    let b = b.skip_initial_state("main");
    b.build()
}

pub fn run() {
    #[cfg(windows)]
    media_session::set_app_id("lb.imad.topnote");
    #[cfg(target_os = "linux")]
    prefer_x11();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| window::show(app)))
        .plugin(window_state_plugin())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            // Carry settings, cache and stats over from the pre-rename identifier.
            std::fs::create_dir_all(&config_dir)?;
            migrate_legacy_dir(&config_dir);
            logfile::init(&config_dir);
            secrets::init(&config_dir);
            let settings = settings::load(&config_dir);
            let conn = lyrics::cache::open(&config_dir.join("lyrics.db"))?;
            let http = reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .timeout(Duration::from_secs(8))
                .connect_timeout(Duration::from_secs(4))
                .gzip(true)
                .build()?;
            let st = Arc::new(AppState {
                http,
                config_dir,
                settings: Mutex::new(settings),
                auth: Default::default(),
                poll_now: Default::default(),
                backoff_until: Mutex::new(None),
                playback: Mutex::new(Default::default()),
                smtc: Mutex::new(None),
                lyrics: lyrics::LyricsService::new(conn),
                window_visible: AtomicBool::new(true),
                listen: Mutex::new(None),
                premium: Mutex::new(None),
                scopes: Mutex::new(None),
                sdk: Mutex::new(Default::default()),
                sdk_changed: Default::default(),
            });
            app.manage(st.clone());

            let handle = app.handle().clone();
            window::setup_tray(&handle)?;
            window::setup_main(&handle, st.clone());
            // Linux: the main window is created hidden (tauri.linux.conf.json). GTK maps a visible window
            // before it applies `decorations: false`, and the window manager then keeps the titlebar's height
            // as extra window and reports bogus frame extents from then on.
            #[cfg(target_os = "linux")]
            window::linux_show_main(&handle, &st.config_dir);
            #[cfg(any(windows, target_os = "linux"))]
            topmost::install(&handle);
            #[cfg(windows)]
            taskbar::install(&handle, st.settings().taskbar_buttons);
            #[cfg(windows)]
            media_session::install(&handle);
            window::register_shortcuts(&handle, &st);
            playback::smtc::spawn(st.clone());
            playback::spawn(handle, st);
            Ok(())
        })
        .on_page_load(|webview, payload| {
            #[cfg(target_os = "linux")]
            if webview.label() == "main" && matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                window::linux_settle(webview.app_handle());
            }
            #[cfg(not(target_os = "linux"))]
            let _ = (webview, payload);
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_settings,
            commands::auth_status,
            commands::login,
            commands::logout,
            commands::set_sp_dc,
            commands::get_playback,
            commands::control,
            commands::get_lyrics,
            commands::provider_health,
            commands::reset_provider_health,
            commands::cache_stats,
            commands::clear_cache,
            commands::set_click_through,
            commands::open_settings,
            commands::open_config_dir,
            commands::hide_main,
            commands::quit,
            commands::translate_lyrics,
            commands::search_lyrics,
            commands::pick_lyrics,
            commands::get_queue,
            commands::listening_stats,
            commands::reset_stats,
            commands::player_token,
            commands::player_event,
            commands::show_spotify,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, ev| {
            if let tauri::RunEvent::Exit = ev {
                stats::flush_current(&app.state::<Arc<AppState>>());
                // don't leave a Spotify running that nobody can see
                playback::desktop::restore_hidden();
            }
        });
}
