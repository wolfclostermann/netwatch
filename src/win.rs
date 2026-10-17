//! Small Win32 helpers.

use std::ffi::c_void;
use std::io;
use std::ptr;

use windows_sys::Win32::Foundation::{ERROR_SUCCESS, HANDLE_FLAG_INHERIT, SetHandleInformation};
use windows_sys::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE,
    STD_OUTPUT_HANDLE,
};
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};

const PERSONALIZE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

/// A NUL-terminated UTF-16 copy of `s`.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub fn check(err: u32) -> io::Result<()> {
    if err == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(err as i32))
    }
}

/// Release builds use the GUI subsystem so no console window appears; reattach
/// to the launching terminal so CLI output is visible there.
pub fn attach_console() {
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

/// Stops child processes inheriting our stdio handles, so a long-lived child
/// doesn't hold open a pipe the caller is reading to EOF.
pub fn disinherit_stdio() {
    for id in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        unsafe { SetHandleInformation(GetStdHandle(id), HANDLE_FLAG_INHERIT, 0) };
    }
}

/// Whether the taskbar uses the light theme. Defaults to dark, as Windows does.
pub fn light_taskbar() -> bool {
    let (key, value) = (wide(PERSONALIZE_KEY), wide("SystemUsesLightTheme"));
    let mut data: u32 = 0;
    let mut size = size_of::<u32>() as u32;
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            ptr::null_mut(),
            &mut data as *mut u32 as *mut c_void,
            &mut size,
        )
    };
    err == ERROR_SUCCESS && data != 0
}
