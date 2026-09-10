use crate::win::wide;
use std::mem::size_of;
use std::path::Path;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegQueryValueExW, RegSetValueExW,
};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "KRun";
const LEGACY_VALUE_NAME: &str = "QuickLaunch";

struct RegistryKey(HKEY);

impl Drop for RegistryKey {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { RegCloseKey(self.0) };
        }
    }
}

pub fn expected_command(executable: &Path) -> String {
    format!("\"{}\" --startup", executable.to_string_lossy())
}

pub fn is_enabled(executable: &Path) -> bool {
    query(VALUE_NAME).is_ok_and(|value| value == expected_command(executable))
}

pub fn set_enabled(executable: &Path, enabled: bool) -> Result<(), String> {
    let key = open_key(KEY_SET_VALUE | KEY_QUERY_VALUE)?;
    if enabled {
        set_value(&key, VALUE_NAME, &expected_command(executable))?;
        remove_legacy_value(&key)?;
    } else {
        delete_value(&key, VALUE_NAME)?;
        remove_legacy_value(&key)?;
    }
    Ok(())
}

fn remove_legacy_value(key: &RegistryKey) -> Result<(), String> {
    if query_with_key(key, LEGACY_VALUE_NAME)
        .is_ok_and(|value| value.to_ascii_lowercase().contains("quicklaunch.exe"))
    {
        delete_value(key, LEGACY_VALUE_NAME)?;
    }
    Ok(())
}

fn set_value(key: &RegistryKey, name: &str, value: &str) -> Result<(), String> {
    let name = wide(name);
    let value = wide(value);
    let result = unsafe {
        RegSetValueExW(
            key.0,
            name.as_ptr(),
            0,
            REG_SZ,
            value.as_ptr().cast(),
            (value.len() * size_of::<u16>()) as u32,
        )
    };
    if result == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("写入开机启动项失败（系统错误 {result}）"))
    }
}

fn delete_value(key: &RegistryKey, name: &str) -> Result<(), String> {
    let name = wide(name);
    let result = unsafe { RegDeleteValueW(key.0, name.as_ptr()) };
    if result == ERROR_SUCCESS || result == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        Err(format!("删除开机启动项失败（系统错误 {result}）"))
    }
}

fn query(name: &str) -> Result<String, String> {
    let key = open_key(KEY_QUERY_VALUE)?;
    query_with_key(&key, name)
}

fn query_with_key(key: &RegistryKey, name: &str) -> Result<String, String> {
    let name = wide(name);
    let mut kind = 0u32;
    let mut bytes = 0u32;
    let first = unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            null(),
            &mut kind,
            null_mut(),
            &mut bytes,
        )
    };
    if first != ERROR_SUCCESS || kind != REG_SZ || bytes < 2 {
        return Err("开机启动项不存在".into());
    }
    let mut buffer = vec![0u16; bytes.div_ceil(2) as usize];
    let second = unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            null(),
            &mut kind,
            buffer.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if second != ERROR_SUCCESS {
        return Err(format!("读取开机启动项失败（系统错误 {second}）"));
    }
    let length = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(buffer.len());
    Ok(String::from_utf16_lossy(&buffer[..length]))
}

fn open_key(access: u32) -> Result<RegistryKey, String> {
    let subkey = wide(RUN_KEY);
    let mut key = null_mut();
    let result = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            null(),
            REG_OPTION_NON_VOLATILE,
            access,
            null(),
            &mut key,
            null_mut(),
        )
    };
    if result == ERROR_SUCCESS {
        Ok(RegistryKey(key))
    } else {
        Err(format!("打开开机启动设置失败（系统错误 {result}）"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_quotes_portable_path() {
        assert_eq!(
            expected_command(Path::new(r"C:\Apps Folder\KRun.exe")),
            r#""C:\Apps Folder\KRun.exe" --startup"#
        );
    }
}
