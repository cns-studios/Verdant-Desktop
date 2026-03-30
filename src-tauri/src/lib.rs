mod auth;
mod background_sync;
mod commands;
mod crypto;
mod db;
mod gmail;
mod imap_sync;
mod mime;
mod smtp_send;
mod state;

use std::collections::HashMap;
use std::sync::Arc;

use db::init_db;
use rusqlite::Connection;
use state::DbState;
use tauri::Manager;
use tauri::{tray::{TrayIconBuilder, TrayIconEvent}, menu::{MenuBuilder, MenuItemBuilder}};
use tokio::sync::Mutex;

use commands::app_config::{AppConfig, AppConfigState, update_app_config};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--update") {
        tauri::async_runtime::block_on(commands::updater::handle_cli_update());
        return;
    }

    let _ = dotenvy::from_filename("../.env").or_else(|_| dotenvy::from_filename(".env"));

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--autostart"])))
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to resolve app data dir");
            std::fs::create_dir_all(&data_dir).expect("Failed to create app data dir");
            let db_path = data_dir.join("emails.db");

            let conn = Connection::open(&db_path).expect("Failed to open DB");
            init_db(&conn).expect("Failed to init DB");

            
            let initial_active_id = db::get_active_account(&conn)
                .ok()
                .flatten()
                .map(|a| a.id)
                .unwrap_or(0);

            let state = Arc::new(DbState {
                conn: Mutex::new(conn),
                tokens: Mutex::new(HashMap::new()),
                active_account_id: Mutex::new(initial_active_id),
                sync_handles: Mutex::new(HashMap::new()),
            });

            app.manage(state.clone());
            app.manage(AppConfigState(Mutex::new(AppConfig { run_in_background: true })));

            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            
            let state_for_sync = state.clone();
            tauri::async_runtime::spawn(async move {
                background_sync::start_all_sync_tasks(state_for_sync).await;
            });
            
            // Tray
            let quit_i = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let show_i = MenuItemBuilder::with_id("show", "Show Verdant").build(app)?;
            let menu = MenuBuilder::new(app)
                .item(&show_i)
                .item(&quit_i)
                .build()?;
            
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .on_menu_event(|app: &tauri::AppHandle, event| {
                    match event.id.as_ref() {
                        "quit" => { app.exit(0); }
                        "show" => { if let Some(w) = app.get_webview_window("main") { let _ = w.show().unwrap(); let _ = w.set_focus().unwrap(); } }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray: &tauri::tray::TrayIcon, event| {
                    if let TrayIconEvent::Click { .. } = event {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show().unwrap();
                            let _ = w.set_focus().unwrap();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppConfigState>();
                let run_in_background = tauri::async_runtime::block_on(async {
                    let s = state.0.lock().await;
                    s.run_in_background
                });

                if run_in_background && window.label() == "main" {
                    window.hide().unwrap();
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            update_app_config,
            
            commands::auth::connect_gmail,
            commands::auth::auth_status,
            commands::auth::logout,
            commands::auth::get_user_profile,
            
            commands::accounts::list_accounts,
            commands::accounts::switch_account,
            commands::accounts::remove_account,
            commands::accounts::add_gmail_account,
            commands::accounts::add_imap_account,
            commands::accounts::add_gmx_account,
            commands::accounts::test_imap_credentials,
            commands::accounts::get_active_account_info,
            
            commands::mail::sync_emails,
            commands::mail::sync_mailbox,
            commands::mail::sync_mailbox_page,
            commands::mail::get_emails,
            commands::mail::set_email_read_status,
            commands::mail::toggle_starred,
            commands::mail::archive_email,
            commands::mail::trash_email,
            commands::mail::permanent_delete_email,
            commands::mail::restore_from_trash,
            commands::mail::sync_imap_mailbox_page,
            commands::mail::deep_search_emails,
            commands::mail::get_mailbox_counts,
            commands::mail::clear_local_data,
            commands::mail::get_inbox_threads,
            commands::mail::get_thread_messages,
            commands::mail::mark_thread_read,
            
            commands::compose::send_email,
            commands::compose::save_draft,
            commands::compose::send_existing_draft,
            
            commands::attachments::download_attachment,
            
            commands::updater::check_for_updates,
            commands::updater::download_latest_update,
            commands::updater::install_and_relaunch,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
