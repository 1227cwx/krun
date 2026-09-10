use crate::config::HotkeyConfig;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, VK_F1, VK_F24,
};

pub const MOD_NOREPEAT_VALUE: u32 = 0x4000;

pub fn is_modifier(key: u32) -> bool {
    matches!(key, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0..=0xA5)
}

pub fn is_supported_key(key: u32) -> bool {
    (b'A' as u32..=b'Z' as u32).contains(&key)
        || (b'0' as u32..=b'9' as u32).contains(&key)
        || (VK_F1 as u32..=VK_F24 as u32).contains(&key)
}

pub fn is_valid(hotkey: &HotkeyConfig) -> bool {
    hotkey.modifiers & (MOD_ALT | MOD_CONTROL | MOD_SHIFT | MOD_WIN) != 0
        && is_supported_key(hotkey.key)
}

pub fn display(hotkey: &HotkeyConfig) -> String {
    let mut parts = Vec::new();
    if hotkey.modifiers & MOD_CONTROL != 0 {
        parts.push("Ctrl".to_string());
    }
    if hotkey.modifiers & MOD_ALT != 0 {
        parts.push("Alt".to_string());
    }
    if hotkey.modifiers & MOD_SHIFT != 0 {
        parts.push("Shift".to_string());
    }
    if hotkey.modifiers & MOD_WIN != 0 {
        parts.push("Win".to_string());
    }
    parts.push(key_name(hotkey.key));
    parts.join(" + ")
}

pub fn key_name(key: u32) -> String {
    if (b'A' as u32..=b'Z' as u32).contains(&key) || (b'0' as u32..=b'9' as u32).contains(&key) {
        return char::from_u32(key).unwrap_or('?').to_string();
    }
    if (VK_F1 as u32..=VK_F24 as u32).contains(&key) {
        return format!("F{}", key - VK_F1 as u32 + 1);
    }
    "未知".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_hotkey_in_stable_order() {
        let value = HotkeyConfig {
            modifiers: MOD_CONTROL | MOD_SHIFT,
            key: b'K' as u32,
        };
        assert_eq!(display(&value), "Ctrl + Shift + K");
        assert!(is_valid(&value));
    }

    #[test]
    fn rejects_bare_key_and_modifier_key() {
        assert!(!is_valid(&HotkeyConfig {
            modifiers: 0,
            key: b'Q' as u32,
        }));
        assert!(is_modifier(0x11));
        assert!(!is_supported_key(0x11));
    }
}
