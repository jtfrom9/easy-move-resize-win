//! Optional "start at login" via the per-user Run registry key
//! (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`). No admin required;
//! the value simply names this executable so the shell launches it at logon.

use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RRF_RT_REG_SZ,
};

const RUN_SUBKEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const VALUE_NAME: PCWSTR = w!("EasyMoveResize");

/// The Run-value command for `exe`, quoted so a path containing spaces still
/// parses as a single argument.
fn command_string(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(once(0)).collect()
}

/// Whether our Run value currently exists (i.e. launch-at-login is on).
pub fn is_enabled() -> bool {
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_SUBKEY,
            VALUE_NAME,
            RRF_RT_REG_SZ,
            None,
            None,
            None,
        ) == ERROR_SUCCESS
    }
}

/// Add the Run value pointing at the current executable. Returns false on error.
pub fn enable() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let data = wide(&command_string(&exe));
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_SUBKEY, 0, KEY_SET_VALUE, &mut hkey)
            != ERROR_SUCCESS
        {
            return false;
        }
        // Reinterpret the UTF-16 buffer (incl. terminator) as bytes for the API.
        let bytes = std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2);
        let ok = RegSetValueExW(hkey, VALUE_NAME, 0, REG_SZ, Some(bytes)) == ERROR_SUCCESS;
        let _ = RegCloseKey(hkey);
        ok
    }
}

/// Remove the Run value. Returns false on error.
pub fn disable() -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_SUBKEY, 0, KEY_SET_VALUE, &mut hkey)
            != ERROR_SUCCESS
        {
            return false;
        }
        let ok = RegDeleteValueW(hkey, VALUE_NAME) == ERROR_SUCCESS;
        let _ = RegCloseKey(hkey);
        ok
    }
}

/// Flip launch-at-login on or off.
pub fn toggle() {
    if is_enabled() {
        disable();
    } else {
        enable();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn command_string_is_quoted() {
        let p = PathBuf::from(r"C:\Program Files\emr\easy-move-resize.exe");
        assert_eq!(
            command_string(&p),
            r#""C:\Program Files\emr\easy-move-resize.exe""#
        );
    }
}
