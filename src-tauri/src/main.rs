#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod commands;
mod credentials;
mod db;
mod imap;
mod models;
mod smtp;
mod sync;

use std::sync::Arc;
use commands::AppDb;
use dirs::data_dir;

#[tokio::main]
async fn main() {
    env_logger::init();

    let data_path = data_dir()
        .expect("could not resolve data directory")
        .join("verdant-mail");

    std::fs::create_dir_all(&data_path).expect("failed to create data dir");
    let db_path = data_path.join("mail.db");

    let pool = db::open(&db_path)
        .await
        .expect("failed to open database");

    let pool = Arc::new(pool);

    tauri::Builder::default()
        .manage(AppDb(pool.clone()))
        .invoke_handler(tauri::generate_handler![
            commands::add_account,
            commands::list_accounts,
            commands::delete_account,
            commands::list_mailboxes,
            commands::list_messages,
            commands::get_message,
            commands::fetch_message_body,
            commands::sync_account,
            commands::search_messages,
            commands::send_email,
            commands::mark_read,
            commands::save_draft,
        ])
        .setup(move |app| {
            let app_handle = app.handle();
            let pool_clone = pool.clone();

            sync::spawn_sync_loop(app_handle, pool_clone);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
