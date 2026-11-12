use std::env;
use std::path::PathBuf;

pub(crate) fn downloads_dir() -> Result<PathBuf, String> {
    if cfg!(target_os = "windows") {
        if let Ok(base) = env::var("USERPROFILE") { return Ok(PathBuf::from(base).join("Downloads")); }
    } else if let Ok(base) = env::var("HOME") {
        return Ok(PathBuf::from(base).join("Downloads"));
    }
    env::current_dir().map_err(|e| e.to_string()).map(|p| p.join("downloads"))
}

pub(crate) fn install_update_sync(path: &str, name: &str) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        if name.ends_with(".pacman") || name.ends_with(".pkg.tar.zst") {
            return run_elevated_command("/usr/bin/pacman", &["-U", "--noconfirm", path]);
        }
        if name.ends_with(".deb") {
            match run_elevated_command("apt-get", &["install", "-y", path]) {
                Ok(_) => return Ok(()),
                Err(_) => return run_elevated_command("dpkg", &["-i", path]),
            }
        }
        if name.ends_with(".rpm") {
            match run_elevated_command("dnf", &["install", "-y", "--allowerasing", path]) {
                Ok(_) => return Ok(()),
                Err(_) => return run_elevated_command("rpm", &["-Uvh", "--replacepkgs", path]),
            }
        }
        if name.ends_with(".appimage") {
            // current_exe() points inside the read-only AppImage mount; $APPIMAGE is the real file.
            let target = appimage_path().ok_or("Not running from an AppImage; cannot replace it in place")?;
            let tmp = target.with_extension("update-tmp");
            std::fs::copy(path, &tmp).map_err(|e| e.to_string())?;
            std::fs::set_permissions(&tmp, std::os::unix::fs::PermissionsExt::from_mode(0o755)).map_err(|e| e.to_string())?;
            // rename (not copy) so a running executable doesn't give ETXTBSY
            std::fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    #[cfg(target_os = "windows")]
    {
        if name.ends_with(".exe") || name.ends_with(".msi") {
            std::process::Command::new("powershell")
                .args(["-Command", &format!("Start-Process -FilePath '{}' -Verb RunAs", path)])
                .spawn().map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    Err(format!("No installer handler for: {}", name))
}

#[cfg(target_os = "linux")]
pub(crate) fn run_elevated_command(bin: &str, args: &[&str]) -> Result<(), String> {
    let is_root = unsafe { libc::getuid() == 0 };
    if is_root {
        let status = std::process::Command::new(bin).args(args).status().map_err(|e| e.to_string())?;
        if status.success() { return Ok(()); }
        return Err(format!("Command failed even as root: {}", bin));
    }

    let status = std::process::Command::new("pkexec").arg(bin).args(args).status();
    if let Ok(s) = status {
        if s.success() { return Ok(()); }
    }

    let terminals = [
        ("gnome-terminal", vec!["--", "bash", "-c"]),
        ("konsole", vec!["-e", "bash", "-c"]),
        ("xfce4-terminal", vec!["-e", "bash", "-c"]),
        ("xterm", vec!["-e", "bash", "-c"]),
        ("alacritty", vec!["-e", "bash", "-c"]),
        ("kitty", vec!["bash", "-c"]),
    ];

    let quoted: Vec<String> = std::iter::once(bin).chain(args.iter().copied())
        .map(|a| format!("'{}'", a.replace('\'', "'\\''")))
        .collect();
    // Propagate sudo's exit status; otherwise the trailing `read` makes failures look like success.
    let full_cmd = format!(
        "sudo {}; status=$?; echo \"Done (exit $status). Press Enter to close...\"; read; exit $status",
        quoted.join(" ")
    );

    for (term, exec_args) in terminals {
        let mut cmd = std::process::Command::new(term);
        for arg in exec_args { cmd.arg(arg); }
        cmd.arg(&full_cmd);

        if let Ok(mut child) = cmd.spawn() {
            let s = child.wait().map_err(|e| e.to_string())?;
            if s.success() { return Ok(()); }
        }
    }

    Err("Could not elevate privileges. Please ensure pkexec is configured or run the app with sudo --update".to_string())
}

#[cfg(target_os = "linux")]
fn appimage_path() -> Option<PathBuf> {
    env::var_os("APPIMAGE").map(PathBuf::from).filter(|p| p.is_file())
}

/// Start a detached helper that waits for this process to exit, then launches the
/// (now updated) app. Waiting matters because of the single-instance plugin: a new
/// instance started while this one is alive would just hand off and quit.
/// Not supported on Windows, where the installer runs separately and the app just exits.
#[cfg(unix)]
pub(crate) fn spawn_relaunch() -> Result<(), String> {
    use std::process::{Command, Stdio};

    #[cfg(target_os = "linux")]
    let target = appimage_path();
    #[cfg(not(target_os = "linux"))]
    let target: Option<PathBuf> = None;

    let target = match target {
        Some(t) => t,
        None => {
            let exe = env::current_exe().map_err(|e| e.to_string())?;
            // After a package upgrade the old inode is gone and Linux reports "<path> (deleted)".
            let raw = exe.to_string_lossy();
            PathBuf::from(raw.strip_suffix(" (deleted)").unwrap_or(&raw).to_string())
        }
    };

    let script = r#"while kill -0 "$1" 2>/dev/null; do sleep 0.2; done; sleep 0.5; exec "$2""#;
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(script).arg("sh").arg(std::process::id().to_string()).arg(&target)
        .args(env::args_os().skip(1).filter(|a| a != "--update"))
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    cmd.spawn().map(|_| ()).map_err(|e| format!("Failed to schedule relaunch: {e}"))
}

#[cfg(not(unix))]
pub(crate) fn spawn_relaunch() -> Result<(), String> { Ok(()) }
