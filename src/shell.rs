use crate::config::LaunchItem;
use crate::win::wide;
use std::mem::{size_of, zeroed};
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::HWND as WindowsHwnd;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, CoUninitialize,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FOS_ALLOWMULTISELECT, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_NODEREFERENCELINKS,
    FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, GlobalFree, HWND};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
use windows_sys::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

pub fn item_from_path(path: PathBuf) -> LaunchItem {
    let name = path
        .file_stem()
        .or_else(|| path.file_name())
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let path_text = path.to_string_lossy().into_owned();
    LaunchItem {
        id: make_id(&path_text),
        name,
        path: path_text,
        icon_path: String::new(),
        arguments: String::new(),
        working_directory: String::new(),
    }
}

fn make_id(path: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.to_lowercase().hash(&mut hasher);
    format!("item-{:016x}", hasher.finish())
}

pub fn choose_icon_file(owner: HWND) -> Result<Option<PathBuf>, String> {
    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if initialized.is_err() {
        return Err(format!("初始化文件选择器失败：{initialized:?}"));
    }
    let result = unsafe { choose_icon_file_inner(owner) };
    unsafe { CoUninitialize() };
    result
}

unsafe fn choose_icon_file_inner(owner: HWND) -> Result<Option<PathBuf>, String> {
    let dialog: IFileOpenDialog = unsafe {
        CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| format!("创建文件选择器失败：{error}"))?
    };
    let options = unsafe {
        dialog
            .GetOptions()
            .map_err(|error| format!("读取选择器设置失败：{error}"))?
    } | FOS_FORCEFILESYSTEM
        | FOS_PATHMUSTEXIST
        | FOS_FILEMUSTEXIST
        | FOS_NODEREFERENCELINKS;
    let filter_name = windows::core::HSTRING::from("图标来源 (*.ico;*.exe)");
    let filter_spec = windows::core::HSTRING::from("*.ico;*.exe");
    let title = windows::core::HSTRING::from("选择图标来源");
    unsafe {
        dialog
            .SetOptions(options)
            .map_err(|error| format!("设置文件选择器失败：{error}"))?;
        dialog
            .SetFileTypes(&[COMDLG_FILTERSPEC {
                pszName: windows::core::PCWSTR(filter_name.as_ptr()),
                pszSpec: windows::core::PCWSTR(filter_spec.as_ptr()),
            }])
            .map_err(|error| format!("设置图标文件筛选失败：{error}"))?;
        dialog
            .SetTitle(windows::core::PCWSTR(title.as_ptr()))
            .map_err(|error| format!("设置选择器标题失败：{error}"))?;
    }
    if unsafe { dialog.Show(Some(WindowsHwnd(owner))) }.is_err() {
        return Ok(None);
    }
    let item = unsafe {
        dialog
            .GetResult()
            .map_err(|error| format!("读取选择结果失败：{error}"))?
    };
    let path = unsafe {
        item.GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|error| format!("读取文件路径失败：{error}"))?
    };
    let text = unsafe { path.to_string() }.map_err(|error| format!("转换文件路径失败：{error}"));
    unsafe { CoTaskMemFree(Some(path.as_ptr().cast())) };
    let path = PathBuf::from(text?);
    let supported = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "ico" | "exe"));
    if !supported {
        return Err("请选择 .ico 或 .exe 图标来源文件。".into());
    }
    Ok(Some(path))
}

pub fn choose_paths(owner: HWND, folders: bool) -> Result<Vec<PathBuf>, String> {
    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if initialized.is_err() {
        return Err(format!("初始化文件选择器失败：{initialized:?}"));
    }
    let result = unsafe { choose_paths_inner(owner, folders) };
    unsafe { CoUninitialize() };
    result
}

unsafe fn choose_paths_inner(owner: HWND, folders: bool) -> Result<Vec<PathBuf>, String> {
    let dialog: IFileOpenDialog = unsafe {
        CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| format!("创建文件选择器失败：{error}"))?
    };
    let mut options = unsafe {
        dialog
            .GetOptions()
            .map_err(|error| format!("读取选择器设置失败：{error}"))?
    };
    options |= FOS_FORCEFILESYSTEM | FOS_ALLOWMULTISELECT | FOS_PATHMUSTEXIST;
    if folders {
        options |= FOS_PICKFOLDERS;
    } else {
        options |= FOS_FILEMUSTEXIST | FOS_NODEREFERENCELINKS;
    }
    unsafe {
        dialog
            .SetOptions(options)
            .map_err(|error| format!("设置文件选择器失败：{error}"))?;
    }
    if unsafe { dialog.Show(Some(WindowsHwnd(owner))) }.is_err() {
        return Ok(Vec::new());
    }
    let results = unsafe {
        dialog
            .GetResults()
            .map_err(|error| format!("读取选择结果失败：{error}"))?
    };
    let count = unsafe {
        results
            .GetCount()
            .map_err(|error| format!("读取选择数量失败：{error}"))?
    };
    let mut paths = Vec::with_capacity(count as usize);
    for index in 0..count {
        let item = unsafe {
            results
                .GetItemAt(index)
                .map_err(|error| format!("读取选择项目失败：{error}"))?
        };
        let path = unsafe {
            item.GetDisplayName(SIGDN_FILESYSPATH)
                .map_err(|error| format!("读取文件路径失败：{error}"))?
        };
        let text =
            unsafe { path.to_string() }.map_err(|error| format!("转换文件路径失败：{error}"));
        unsafe { CoTaskMemFree(Some(path.as_ptr().cast())) };
        paths.push(PathBuf::from(text?));
    }
    Ok(paths)
}

pub fn run_as_admin(owner: HWND, item: &LaunchItem) -> Result<(), String> {
    launch_with_verb(owner, item, "runas")
}

fn launch_with_verb(owner: HWND, item: &LaunchItem, verb_text: &str) -> Result<(), String> {
    let path = expand_environment(&item.path);
    let file = wide(&path);
    let verb = wide(verb_text);
    let parameters = wide(&item.arguments);
    let directory = if item.working_directory.trim().is_empty() {
        Path::new(&path)
            .parent()
            .map(|path| wide(path.as_os_str()))
            .unwrap_or_else(|| vec![0])
    } else {
        wide(expand_environment(&item.working_directory))
    };
    let mut info: SHELLEXECUTEINFOW = unsafe { zeroed() };
    info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.hwnd = owner;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = parameters.as_ptr();
    info.lpDirectory = directory.as_ptr();
    info.nShow = SW_SHOWNORMAL;
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        return Err(format!(
            "无法启动“{}”（系统错误 {}）",
            item.name,
            unsafe { GetLastError() }
        ));
    }
    if !info.hProcess.is_null() {
        unsafe { CloseHandle(info.hProcess) };
    }
    Ok(())
}

pub fn open_with(owner: HWND, path: &str) -> Result<(), String> {
    let item = LaunchItem {
        name: "打开方式".into(),
        path: "rundll32.exe".into(),
        arguments: format!("shell32.dll,OpenAs_RunDLL \"{}\"", expand_environment(path)),
        ..LaunchItem::default()
    };
    launch(owner, &item)
}

pub fn copy_path(owner: HWND, path: &str) -> Result<(), String> {
    let value = wide(expand_environment(path));
    if unsafe { OpenClipboard(owner) } == 0 {
        return Err("无法打开剪贴板".into());
    }
    unsafe { EmptyClipboard() };
    let bytes = value.len() * size_of::<u16>();
    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) };
    if memory.is_null() {
        unsafe { CloseClipboard() };
        return Err("无法分配剪贴板内存".into());
    }
    let target = unsafe { GlobalLock(memory) };
    if target.is_null() {
        unsafe {
            GlobalFree(memory);
            CloseClipboard();
        }
        return Err("无法写入剪贴板".into());
    }
    unsafe {
        std::ptr::copy_nonoverlapping(value.as_ptr().cast::<u8>(), target.cast(), bytes);
        GlobalUnlock(memory);
    }
    if unsafe { SetClipboardData(CF_UNICODETEXT as u32, memory) }.is_null() {
        unsafe {
            GlobalFree(memory);
            CloseClipboard();
        }
        return Err("无法设置剪贴板内容".into());
    }
    unsafe { CloseClipboard() };
    Ok(())
}

pub fn launch(owner: HWND, item: &LaunchItem) -> Result<(), String> {
    launch_with_verb(owner, item, "open")
}

pub fn reveal(owner: HWND, path: &str) -> Result<(), String> {
    let explorer = wide("explorer.exe");
    let quoted = format!("/select,\"{}\"", expand_environment(path));
    let arguments = wide(&quoted);
    let verb = wide("open");
    let mut info: SHELLEXECUTEINFOW = unsafe { zeroed() };
    info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
    info.hwnd = owner;
    info.lpVerb = verb.as_ptr();
    info.lpFile = explorer.as_ptr();
    info.lpParameters = arguments.as_ptr();
    info.nShow = SW_SHOWNORMAL;
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        Err("无法打开文件所在位置".into())
    } else {
        Ok(())
    }
}

pub fn open_directory(owner: HWND, directory: &Path) -> Result<(), String> {
    let item = LaunchItem {
        name: "配置目录".into(),
        path: directory.to_string_lossy().into_owned(),
        ..LaunchItem::default()
    };
    launch(owner, &item)
}

pub fn expand_environment(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut remaining = value;
    while let Some(start) = remaining.find('%') {
        output.push_str(&remaining[..start]);
        let after_start = &remaining[start + 1..];
        let Some(end) = after_start.find('%') else {
            output.push_str(&remaining[start..]);
            return output;
        };
        let variable = &after_start[..end];
        match std::env::var(variable) {
            Ok(content) => output.push_str(&content),
            Err(_) => output.push_str(&remaining[start..start + end + 2]),
        }
        remaining = &after_start[end + 1..];
    }
    output.push_str(remaining);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_name_comes_from_file_stem() {
        let item = item_from_path(PathBuf::from(r"C:\Apps\Editor.exe"));
        assert_eq!(item.name, "Editor");
        assert!(!item.id.is_empty());
    }
}
