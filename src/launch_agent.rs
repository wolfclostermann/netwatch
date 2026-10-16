//! Start-at-login support via a per-user LaunchAgent.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs, io};

const LABEL: &str = "local.netwatch";

fn home() -> io::Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("HOME is not set"))
}

fn plist_path() -> io::Result<PathBuf> {
    Ok(home()?.join(format!("Library/LaunchAgents/{LABEL}.plist")))
}

fn gui_domain() -> io::Result<String> {
    let out = Command::new("id").arg("-u").output()?;
    let uid = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(format!("gui/{uid}"))
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn plist(exe: &Path, log: &Path) -> String {
    let exe = xml_escape(&exe.to_string_lossy());
    let log = xml_escape(&log.to_string_lossy());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <!-- Restart after a crash, but stay stopped after Quit. -->
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>ProcessType</key>
    <string>Interactive</string>
    <key>StandardErrorPath</key>
    <string>{log}</string>
</dict>
</plist>
"#
    )
}

/// Stops and unregisters the agent if it is loaded. Errors are ignored since it
/// may simply not be loaded.
fn bootout(domain: &str) {
    let _ = Command::new("launchctl")
        .args(["bootout", &format!("{domain}/{LABEL}")])
        .stderr(std::process::Stdio::null())
        .status();
}

pub fn install() -> io::Result<()> {
    let exe = env::current_exe()?.canonicalize()?;
    if exe.components().any(|c| c.as_os_str() == "target") {
        eprintln!(
            "warning: {} is inside a cargo build directory and will move or vanish on \
             `cargo clean`. Consider `cargo install --path .` and running `netwatch install` \
             from ~/.cargo/bin instead.",
            exe.display()
        );
    }

    let path = plist_path()?;
    let log = home()?.join("Library/Logs/netwatch.log");
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(&path, plist(&exe, &log))?;

    let domain = gui_domain()?;
    bootout(&domain);
    let status = Command::new("launchctl")
        .args(["bootstrap", &domain])
        .arg(&path)
        .status()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "launchctl bootstrap failed ({status})"
        )));
    }
    println!(
        "Installed {} — netwatch will start at login.",
        path.display()
    );
    Ok(())
}

pub fn uninstall() -> io::Result<()> {
    bootout(&gui_domain()?);
    let path = plist_path()?;
    match fs::remove_file(&path) {
        Ok(()) => println!("Removed {}.", path.display()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => println!("netwatch was not installed."),
        Err(e) => return Err(e),
    }
    Ok(())
}
