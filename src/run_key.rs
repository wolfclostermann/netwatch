//! Start-at-login support via the per-user `Run` registry key.
//!
//! `install` copies the binary to `%LOCALAPPDATA%\Programs\netwatch` and
//! registers that copy, so rebuilding or `cargo clean` doesn't affect it.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use std::{env, fs, io, thread};

use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW,
};

use crate::win::{check, disinherit_stdio, wide};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "netwatch";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DETACHED_PROCESS: u32 = 0x0000_0008;

fn install_dir() -> io::Result<PathBuf> {
    env::var_os("LOCALAPPDATA")
        .map(|dir| PathBuf::from(dir).join(r"Programs\netwatch"))
        .ok_or_else(|| io::Error::other("LOCALAPPDATA is not set"))
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Stops any other running netwatch. Errors are ignored since none may be running.
fn stop_others() {
    let Ok(exe) = env::current_exe() else { return };
    let Some(name) = exe.file_name() else { return };
    let _ = Command::new("taskkill")
        .args([
            "/F",
            "/FI",
            &format!("PID ne {}", std::process::id()),
            "/IM",
        ])
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

/// Copies `from` over `to`, retrying briefly: a netwatch just stopped by
/// `stop_others` may still have `to` locked for a moment.
fn copy_binary(from: &Path, to: &Path) -> io::Result<()> {
    for _ in 0..10 {
        if fs::copy(from, to).is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(200));
    }
    fs::copy(from, to).map(drop)
}

pub fn install() -> io::Result<()> {
    let exe = env::current_exe()?;
    let dir = install_dir()?;
    let target = dir.join("netwatch.exe");

    stop_others();
    if !same_file(&exe, &target) {
        fs::create_dir_all(&dir)?;
        copy_binary(&exe, &target)?;
        println!("Copied netwatch to {}.", target.display());
    }

    let command = wide(&format!("\"{}\"", target.display()));
    let (key, value) = (wide(RUN_KEY), wide(VALUE));
    check(unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            REG_SZ,
            command.as_ptr().cast(),
            (command.len() * 2) as u32,
        )
    })?;

    disinherit_stdio();
    Command::new(&target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS)
        .spawn()?;
    println!(r"Installed HKCU\{RUN_KEY}\{VALUE} — netwatch will start at login.");
    Ok(())
}

pub fn uninstall() -> io::Result<()> {
    stop_others();
    let (key, value) = (wide(RUN_KEY), wide(VALUE));
    let err = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) };
    if err == ERROR_FILE_NOT_FOUND {
        println!("netwatch was not registered to start at login.");
    } else {
        check(err)?;
        println!(r"Removed HKCU\{RUN_KEY}\{VALUE}.");
    }

    let dir = install_dir()?;
    let target = dir.join("netwatch.exe");
    if !target.exists() {
        return Ok(());
    }
    if same_file(&env::current_exe()?, &target) {
        println!(
            "{} is running this command, so it can't remove itself; delete it by hand.",
            target.display()
        );
        return Ok(());
    }
    // As in `install`, a just-stopped netwatch may hold the file briefly.
    for attempt in 0..10 {
        match fs::remove_file(&target) {
            Ok(()) => break,
            Err(_) if attempt < 9 => thread::sleep(Duration::from_millis(200)),
            Err(e) => return Err(e),
        }
    }
    let _ = fs::remove_dir(&dir);
    println!("Removed {}.", target.display());
    Ok(())
}
