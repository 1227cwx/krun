use crate::{icon_loader, shell, win};
use std::collections::HashSet;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize, IPersistFile, STGM_READ,
};
use windows::Win32::UI::Shell::{IShellLinkW, SLGP_RAWPATH, ShellLink};
use windows::core::{Interface, PCWSTR};
use windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon;

#[derive(Debug, PartialEq, Eq)]
pub struct IconSource {
    pub path: PathBuf,
    pub index: i32,
}

pub fn prepare(selected: &Path, executable_directory: &Path) -> Result<IconSource, String> {
    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    initialized
        .ok()
        .map_err(|error| format!("初始化快捷方式读取失败：{error}"))?;
    let result = prepare_inner(selected, executable_directory);
    unsafe { CoUninitialize() };
    result
}

fn prepare_inner(selected: &Path, directory: &Path) -> Result<IconSource, String> {
    let mut source = resolve(selected, &mut HashSet::new())?;
    let value = source.path.to_string_lossy();
    let icon = icon_loader::load_source_icon(&value, Some(source.index), 32);
    if icon.is_null() {
        return Err("无法从所选文件或快捷方式目标提取图标，原图标未更改。".into());
    }
    unsafe { DestroyIcon(icon) };
    if extension(&source.path) == "ico" {
        source.path = store_ico(&source.path, directory)?;
        source.index = 0;
    }
    Ok(source)
}

fn resolve(path: &Path, seen: &mut HashSet<PathBuf>) -> Result<IconSource, String> {
    let path =
        fs::canonicalize(path).map_err(|error| format!("图标来源不存在或无法读取：{error}"))?;
    if seen.len() >= 16 || !seen.insert(path.clone()) {
        return Err("快捷方式存在循环引用或嵌套过深，原图标未更改。".into());
    }
    match extension(&path).as_str() {
        "ico" | "exe" | "dll" => Ok(IconSource { path, index: 0 }),
        "lnk" => {
            let link: IShellLinkW = unsafe {
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                    .map_err(|error| format!("创建快捷方式读取器失败：{error}"))?
            };
            let persist: IPersistFile = link
                .cast()
                .map_err(|error| format!("读取快捷方式失败：{error}"))?;
            let wide = win::wide(path.as_os_str());
            unsafe { persist.Load(PCWSTR(wide.as_ptr()), STGM_READ) }
                .map_err(|error| format!("无法打开快捷方式：{error}"))?;
            let mut buffer = vec![0u16; 32768];
            let mut index = 0;
            unsafe { link.GetIconLocation(&mut buffer, &mut index) }
                .map_err(|error| format!("无法读取快捷方式图标位置：{error}"))?;
            let location = buffer_text(&buffer);
            let parent = path.parent().ok_or("快捷方式路径无效")?;
            if !location.trim().is_empty() {
                let icon_path = expand_path(&location, parent);
                if extension(&icon_path) == "lnk" {
                    return resolve(&icon_path, seen);
                }
                let icon_path = fs::canonicalize(icon_path)
                    .map_err(|error| format!("快捷方式指定的图标文件无法读取：{error}"))?;
                return Ok(IconSource {
                    path: icon_path,
                    index,
                });
            }
            buffer.fill(0);
            unsafe { link.GetPath(&mut buffer, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32) }
                .map_err(|error| format!("无法读取快捷方式目标：{error}"))?;
            let target = buffer_text(&buffer);
            if target.trim().is_empty() {
                return Err("快捷方式没有可用的文件目标，原图标未更改。".into());
            }
            resolve(&expand_path(&target, parent), seen)
        }
        _ => Err("请选择 .ico、.exe、.dll 或 .lnk 图标来源文件。".into()),
    }
}

fn buffer_text(buffer: &[u16]) -> String {
    let length = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..length])
}

fn expand_path(value: &str, parent: &Path) -> PathBuf {
    let expanded = shell::expand_environment(value.trim().trim_matches('"'));
    let path = PathBuf::from(expanded);
    if path.is_absolute() {
        path
    } else {
        parent.join(path)
    }
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn store_ico(source: &Path, directory: &Path) -> Result<PathBuf, String> {
    let bytes = fs::read(source).map_err(|error| format!("读取图标文件失败：{error}"))?;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hash);
    let icons = directory.join("icons");
    fs::create_dir_all(&icons).map_err(|error| format!("创建图标目录失败：{error}"))?;
    let destination = icons.join(format!("{:016x}.ico", hash.finish()));
    if destination.exists() {
        if fs::read(&destination).map_err(|error| format!("校验已保存图标失败：{error}"))? != bytes
        {
            return Err("已保存图标校验不一致，原图标未更改。".into());
        }
    } else {
        let temporary = icons.join(format!("{:016x}-{}.tmp", hash.finish(), std::process::id()));
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            let from = win::wide(temporary.as_os_str());
            let to = win::wide(destination.as_os_str());
            use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW};
            if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(&temporary);
            return Err(format!("保存图标副本失败：{error}"));
        }
    }
    fs::canonicalize(destination).map_err(|error| format!("解析已保存图标路径失败：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_icon_survives_link_and_original_ico_deletion() {
        let directory = std::env::temp_dir().join(format!(
            "krun-icon-source-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let ico = directory.join("original.ico");
        fs::write(&ico, include_bytes!("../assets/krun.ico")).unwrap();
        let shortcut = directory.join("test.lnk");
        let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        initialized.ok().unwrap();
        {
            let link: IShellLinkW =
                unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }.unwrap();
            let target = win::wide("C:\\Windows\\notepad.exe");
            let location = win::wide(ico.as_os_str());
            unsafe {
                link.SetPath(PCWSTR(target.as_ptr())).unwrap();
                link.SetIconLocation(PCWSTR(location.as_ptr()), 0).unwrap();
            }
            let persist: IPersistFile = link.cast().unwrap();
            let filename = win::wide(shortcut.as_os_str());
            unsafe {
                persist.Save(PCWSTR(filename.as_ptr()), true).unwrap();
            }
        }
        let source = prepare(&shortcut, &directory).unwrap();
        assert_eq!(extension(&source.path), "ico");
        assert!(source.path.parent().unwrap().ends_with("icons"));
        fs::remove_file(shortcut).unwrap();
        fs::remove_file(ico).unwrap();
        let icon =
            icon_loader::load_source_icon(&source.path.to_string_lossy(), Some(source.index), 32);
        assert!(!icon.is_null());
        unsafe {
            DestroyIcon(icon);
            CoUninitialize();
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn shortcut_target_and_resource_index_survive_shortcut_deletion() {
        let directory = std::env::temp_dir().join(format!(
            "krun-shortcut-target-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let windows_directory = std::env::var("WINDIR").unwrap();
        let target = PathBuf::from(&windows_directory).join("System32\\notepad.exe");
        let library = PathBuf::from(&windows_directory).join("System32\\shell32.dll");
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .unwrap();
        for custom in [false, true] {
            let shortcut = directory.join(if custom { "custom.lnk" } else { "target.lnk" });
            {
                let link: IShellLinkW =
                    unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }.unwrap();
                let target_name = win::wide(target.as_os_str());
                unsafe { link.SetPath(PCWSTR(target_name.as_ptr())) }.unwrap();
                if custom {
                    let location = win::wide(library.as_os_str());
                    unsafe { link.SetIconLocation(PCWSTR(location.as_ptr()), 3) }.unwrap();
                }
                let persist: IPersistFile = link.cast().unwrap();
                let filename = win::wide(shortcut.as_os_str());
                unsafe { persist.Save(PCWSTR(filename.as_ptr()), true) }.unwrap();
            }
            let source = prepare(&shortcut, &directory).unwrap();
            assert_eq!(
                source.path,
                fs::canonicalize(if custom { &library } else { &target }).unwrap()
            );
            assert_eq!(source.index, if custom { 3 } else { 0 });
            fs::remove_file(shortcut).unwrap();
            let icon = icon_loader::load_source_icon(
                &source.path.to_string_lossy(),
                Some(source.index),
                48,
            );
            assert!(!icon.is_null());
            unsafe { DestroyIcon(icon) };
        }
        unsafe { CoUninitialize() };
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn invalid_source_does_not_create_managed_icon_directory() {
        let directory =
            std::env::temp_dir().join(format!("krun-icon-invalid-{}", std::process::id()));
        assert!(prepare(&directory.join("missing.lnk"), &directory).is_err());
        assert!(!directory.join("icons").exists());
    }
}
