use crate::win::wide;
use std::mem::size_of;
use windows_sys::Win32::Foundation::{COLORREF, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};

const DWM_KEY: &str = "Software\\Microsoft\\Windows\\DWM";
const ACCENT_VALUE: &str = "AccentColor";

/// KRun brand teal, used when Windows does not expose an accent color.
const DEFAULT_ACCENT: COLORREF = 0x00938316;

/// Returns the current Windows accent color as a `COLORREF`.
///
/// Windows stores the accent as `0xAABBGGRR`, which already matches the
/// `0x00BBGGRR` layout of `COLORREF`, so only the alpha byte is dropped.
pub fn accent_color() -> COLORREF {
    let subkey = wide(DWM_KEY);
    let value_name = wide(ACCENT_VALUE);
    let mut data = 0u32;
    let mut bytes = size_of::<u32>() as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            value_name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut data as *mut u32).cast(),
            &mut bytes,
        )
    };
    if result != ERROR_SUCCESS || bytes != size_of::<u32>() as u32 {
        return DEFAULT_ACCENT;
    }
    let color = (data & 0x00FF_FFFF) as COLORREF;
    if color == 0 { DEFAULT_ACCENT } else { color }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_drops_alpha_byte() {
        let packed = 0xFF_93_83_16u32;
        assert_eq!((packed & 0x00FF_FFFF) as COLORREF, 0x0093_8316);
    }

    #[test]
    fn accent_is_reported_or_falls_back() {
        let color = accent_color();
        assert!(color != 0);
    }
}
