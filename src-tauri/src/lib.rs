mod auth;
mod commands;
mod error;
mod lyrics;
mod playback;
mod secrets;
mod settings;
mod state;
mod stats;
#[cfg(windows)]
mod taskbar;
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

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| window::show(app)))
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE | tauri_plugin_window_state::StateFlags::POSITION,
                )
                .with_denylist(&["settings"])
                .build(),
        )
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            // Carry settings, cache and stats over from the pre-rename identifier.
            std::fs::create_dir_all(&config_dir)?;
            migrate_legacy_dir(&config_dir);
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
            });
            app.manage(st.clone());

            let handle = app.handle().clone();
            window::setup_tray(&handle)?;
            window::setup_main(&handle, st.clone());
            #[cfg(windows)]
            taskbar::install(&handle, st.settings().taskbar_buttons);
            window::register_shortcuts(&handle, &st);
            playback::smtc::spawn(st.clone());
            playback::spawn(handle, st);
            Ok(())
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
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, ev| {
            if let tauri::RunEvent::Exit = ev {
                stats::flush_current(&app.state::<Arc<AppState>>());
            }
        });
}
