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
            match run_elevated_command("dnf", &["install", "-y", path]) {
                Ok(_) => return Ok(()),
                Err(_) => return run_elevated_command("rpm", &["-U", path]),
            }
        }
        if name.ends_with(".appimage") {
            std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).map_err(|e| e.to_string())?;
            let current = std::env::current_exe().map_err(|e| e.to_string())?;
            std::fs::copy(path, &current).map_err(|e| e.to_string())?;
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

    let full_cmd = format!("sudo {} {}; echo 'Done. Press Enter to close...'; read", bin, args.join(" "));

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
