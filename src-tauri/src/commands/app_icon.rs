//! Switchable app icon.
//!
//! The pre-made variants are embedded as plain image data, so switching never
//! needs a recompile: the chosen id is persisted in `app-config.json` and applied
//! at runtime to the window, the tray and the OS launcher entries.

use tauri::image::Image;
use tauri::Manager;

use super::app_config::{persist_app_config, AppConfigState};

pub const TRAY_ID: &str = "main";
pub const DEFAULT_ID: &str = "normal";

struct Variant {
    id: &'static str,
    png_32: &'static [u8],
    png_128: &'static [u8],
    png_256: &'static [u8],
    #[allow(dead_code)]
    ico: &'static [u8],
}

macro_rules! variant {
    ($id:literal) => {
        Variant {
            id: $id,
            png_32: include_bytes!(concat!("../../icons/variants/", $id, "-32.png")),
            png_128: include_bytes!(concat!("../../icons/variants/", $id, "-128.png")),
            png_256: include_bytes!(concat!("../../icons/variants/", $id, "-256.png")),
            ico: include_bytes!(concat!("../../icons/variants/", $id, ".ico")),
        }
    };
}

static VARIANTS: &[Variant] = &[
    variant!("normal"),
    variant!("blue"),
    variant!("red"),
    variant!("orange"),
    variant!("yellow"),
    variant!("crt"),
    variant!("frutiger-aero"),
    variant!("sunshine-rainbows"),
];

fn find(id: &str) -> Option<&'static Variant> {
    VARIANTS.iter().find(|v| v.id == id)
}

/// 128px PNG for the given variant id (falls back to the default icon).
pub fn png_128(id: &str) -> &'static [u8] {
    find(id).or_else(|| find(DEFAULT_ID)).map_or(&[], |v| v.png_128)
}

pub fn normalize(raw: &str) -> String {
    find(raw.trim()).map_or(DEFAULT_ID, |v| v.id).to_string()
}

/// Applies the saved icon at startup. The default icon needs no work: it is
/// what the bundle and the executable already ship with.
pub fn apply_saved(app: &tauri::AppHandle) {
    let id = {
        let state = app.state::<AppConfigState>();
        tauri::async_runtime::block_on(async { state.0.lock().await.app_icon.clone() })
    };
    if id != DEFAULT_ID {
        apply(app, &id);
    }
}

fn apply(app: &tauri::AppHandle, id: &str) {
    let Some(variant) = find(id) else { return };

    if let (Some(window), Ok(icon)) = (app.get_webview_window("main"), Image::from_bytes(variant.png_256)) {
        let _ = window.set_icon(icon);
    }
    if let (Some(tray), Ok(icon)) = (app.tray_by_id(TRAY_ID), Image::from_bytes(variant.png_128)) {
        let _ = tray.set_icon(Some(icon));
    }

    let app = app.clone();
    let id = variant.id;
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = platform::apply_system(&app, find(id).unwrap()) {
            eprintln!("[app-icon] failed to update system icon: {e}");
        }
    });
}

#[tauri::command]
pub async fn set_app_icon(
    id: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppConfigState>,
) -> Result<(), String> {
    let variant = find(id.trim()).ok_or_else(|| format!("Unknown app icon: {id}"))?;
    {
        let mut s = state.0.lock().await;
        s.app_icon = variant.id.to_string();
        super::app_config::sync_callback_appearance(&s);
        persist_app_config(&app, &s)?;
    }

    if variant.id == DEFAULT_ID {
        // Restore the bundled icon for the live window and tray.
        if let Some(icon) = app.default_window_icon().cloned() {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_icon(icon.clone());
            }
            if let Some(tray) = app.tray_by_id(TRAY_ID) {
                let _ = tray.set_icon(Some(icon));
            }
        }
        let app2 = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            if let Err(e) = platform::apply_system(&app2, find(DEFAULT_ID).unwrap()) {
                eprintln!("[app-icon] failed to restore system icon: {e}");
            }
        });
    } else {
        apply(&app, variant.id);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{Variant, DEFAULT_ID};
    use std::fs;
    use std::path::PathBuf;

    // Icon names the packages reference: Arch (`verdant`) and deb/rpm (binary name).
    const ICON_NAMES: &[&str] = &["verdant", "Verdant-Desktop", "verdant-desktop"];

    fn data_home() -> Result<PathBuf, String> {
        if let Some(dir) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
            return Ok(PathBuf::from(dir));
        }
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join(".local").join("share"))
            .ok_or_else(|| "$HOME is not set".to_string())
    }

    /// User-level hicolor icons take precedence over the packaged ones in
    /// `/usr/share/icons`, so the launcher, app menu and taskbar pick up the
    /// chosen icon without touching the installation (no root, no rebuild).
    pub fn apply_system(_app: &tauri::AppHandle, variant: &Variant) -> Result<(), String> {
        let theme = data_home()?.join("icons").join("hicolor");
        let sizes: [(u32, &[u8]); 3] = [(32, variant.png_32), (128, variant.png_128), (256, variant.png_256)];

        for (size, bytes) in sizes {
            let dir = theme.join(format!("{size}x{size}")).join("apps");
            for name in ICON_NAMES {
                let path = dir.join(format!("{name}.png"));
                if variant.id == DEFAULT_ID {
                    let _ = fs::remove_file(&path);
                } else {
                    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;
                    fs::write(&path, bytes).map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
                }
            }
        }

        // Invalidate icon caches so shells notice the change right away.
        let _ = std::process::Command::new("gtk-update-icon-cache")
            .args(["-f", "-t"])
            .arg(&theme)
            .output();
        Ok(())
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{Variant, DEFAULT_ID};
    use std::os::windows::process::CommandExt;
    use tauri::Manager;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    // Points every shortcut (.lnk) that launches this executable at the chosen icon:
    // desktop, Start menu and pinned taskbar/start entries. Paths come in through
    // environment variables, never through string interpolation.
    const SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
$exe = $env:VERDANT_EXE
$ico = $env:VERDANT_ICO
$loc = if ($ico) { "$ico,0" } else { "$exe,0" }
$pinned = Join-Path $env:APPDATA 'Microsoft\Internet Explorer\Quick Launch\User Pinned'
$dirs = @(
  [Environment]::GetFolderPath('Desktop'),
  [Environment]::GetFolderPath('CommonDesktopDirectory'),
  [Environment]::GetFolderPath('Programs'),
  [Environment]::GetFolderPath('CommonPrograms'),
  (Join-Path $pinned 'TaskBar'),
  (Join-Path $pinned 'StartMenu')
)
$sh = New-Object -ComObject WScript.Shell
foreach ($d in $dirs) {
  if (-not $d -or -not (Test-Path $d)) { continue }
  Get-ChildItem -Path $d -Filter *.lnk -Recurse -File | ForEach-Object {
    $l = $sh.CreateShortcut($_.FullName)
    if ($l.TargetPath -and ($l.TargetPath -ieq $exe)) { $l.IconLocation = $loc; $l.Save() }
  }
}
Add-Type -Namespace Verdant -Name Shell -MemberDefinition '[DllImport("shell32.dll")] public static extern void SHChangeNotify(int e, uint f, IntPtr a, IntPtr b);'
[Verdant.Shell]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)
"#;

    pub fn apply_system(app: &tauri::AppHandle, variant: &Variant) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| format!("Failed to resolve binary path: {e}"))?;

        let ico_path = if variant.id == DEFAULT_ID {
            None // fall back to the icon embedded in the executable
        } else {
            let dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("Failed to resolve app data dir: {e}"))?
                .join("icons");
            std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create icon dir: {e}"))?;
            let path = dir.join(format!("verdant-{}.ico", variant.id));
            std::fs::write(&path, variant.ico).map_err(|e| format!("Failed to write icon: {e}"))?;
            Some(path)
        };

        let out = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", SCRIPT])
            .env("VERDANT_EXE", &exe)
            .env("VERDANT_ICO", ico_path.as_deref().map(|p| p.as_os_str()).unwrap_or_default())
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("Failed to run PowerShell: {e}"))?;

        if !out.status.success() {
            return Err(format!("PowerShell failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        Ok(())
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
mod platform {
    use super::Variant;

    pub fn apply_system(_app: &tauri::AppHandle, _variant: &Variant) -> Result<(), String> {
        Ok(())
    }
}
