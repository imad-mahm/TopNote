mod auth;
mod commands;
mod error;
mod lyrics;
mod playback;
mod secrets;
mod settings;
mod state;
mod window;

use state::{AppState, USER_AGENT};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::Manager;

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
            std::fs::create_dir_all(&config_dir)?;
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
            });
            app.manage(st.clone());

            let handle = app.handle().clone();
            window::setup_tray(&handle)?;
            window::setup_main(&handle, st.clone());
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
