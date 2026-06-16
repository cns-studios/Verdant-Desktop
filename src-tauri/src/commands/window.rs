#[derive(serde::Serialize)]
pub struct StartupFlags {
    pub is_autostart: bool,
}

#[tauri::command]
pub fn get_startup_flags() -> StartupFlags {
    let args: Vec<String> = std::env::args().collect();
    StartupFlags {
        is_autostart: args.iter().any(|arg| arg == "--autostart"),
    }
}

#[tauri::command]
pub fn hide_main_window(window: tauri::Window) -> Result<(), String> {
    window.hide().map_err(|e| e.to_string())
}
