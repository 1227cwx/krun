use crate::win::wide;
use std::collections::HashSet;
use std::mem::{size_of, zeroed};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::Controls::{IImageList, ILD_TRANSPARENT};
use windows::Win32::UI::Shell::{SHGetImageList, SHIL_EXTRALARGE, SHIL_JUMBO, SHIL_LARGE};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;
use windows_sys::Win32::UI::Shell::{SHFILEINFOW, SHGFI_SYSICONINDEX, SHGetFileInfoW};
use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON, PostMessageW, WM_APP};

pub const ICON_READY_MESSAGE: u32 = WM_APP + 5;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct IconKey {
    pub path: String,
    pub size: i32,
}

struct Request {
    key: IconKey,
}

pub struct ReadyIcon {
    pub key: IconKey,
    icon_value: usize,
}

impl ReadyIcon {
    pub fn into_parts(self) -> (IconKey, HICON) {
        (self.key, self.icon_value as HICON)
    }
}

pub struct IconLoader {
    sender: mpsc::SyncSender<Request>,
    ready: Arc<Mutex<Vec<ReadyIcon>>>,
    pending: HashSet<IconKey>,
}

impl IconLoader {
    pub fn new(hwnd: HWND) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<Request>(128);
        let ready = Arc::new(Mutex::new(Vec::new()));
        let worker_ready = Arc::clone(&ready);
        let hwnd_value = hwnd as usize;
        thread::spawn(move || {
            let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            while let Ok(request) = receiver.recv() {
                let icon = load_icon(&request.key.path, request.key.size);
                if let Ok(mut queue) = worker_ready.lock() {
                    queue.push(ReadyIcon {
                        key: request.key,
                        icon_value: icon as usize,
                    });
                } else if !icon.is_null() {
                    unsafe { DestroyIcon(icon) };
                }
                unsafe {
                    PostMessageW(hwnd_value as HWND, ICON_READY_MESSAGE, 0, 0);
                }
            }
            if initialized.is_ok() {
                unsafe { CoUninitialize() };
            }
        });
        Self {
            sender,
            ready,
            pending: HashSet::new(),
        }
    }

    pub fn request(&mut self, key: IconKey) {
        if self.pending.insert(key.clone())
            && self.sender.try_send(Request { key: key.clone() }).is_err()
        {
            self.pending.remove(&key);
        }
    }

    pub fn drain(&mut self) -> Vec<ReadyIcon> {
        let items = self
            .ready
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default();
        for item in &items {
            self.pending.remove(&item.key);
        }
        items
    }
}

impl Drop for IconLoader {
    fn drop(&mut self) {
        if let Ok(mut queue) = self.ready.lock() {
            for item in queue.drain(..) {
                let (_, icon) = item.into_parts();
                if !icon.is_null() {
                    unsafe { DestroyIcon(icon) };
                }
            }
        }
    }
}

fn load_icon(path: &str, size: i32) -> HICON {
    let path = wide(path);
    let mut info: SHFILEINFOW = unsafe { zeroed() };
    let result = unsafe {
        SHGetFileInfoW(
            path.as_ptr(),
            FILE_ATTRIBUTE_NORMAL,
            &mut info,
            size_of::<SHFILEINFOW>() as u32,
            SHGFI_SYSICONINDEX,
        )
    };
    if result == 0 {
        return std::ptr::null_mut();
    }
    let list_kind = if size <= 32 {
        SHIL_LARGE
    } else if size <= 64 {
        SHIL_EXTRALARGE
    } else {
        SHIL_JUMBO
    };
    let list = unsafe { SHGetImageList::<IImageList>(list_kind as i32) };
    let Ok(list) = list else {
        return std::ptr::null_mut();
    };
    unsafe {
        list.GetIcon(info.iIcon, ILD_TRANSPARENT.0)
            .map(|icon| icon.0)
            .unwrap_or(std::ptr::null_mut())
    }
}

#[cfg(test)]
mod tests {
    use super::IconKey;

    #[test]
    fn icon_cache_key_includes_physical_size() {
        let a = IconKey {
            path: "app.exe".into(),
            size: 32,
        };
        let b = IconKey {
            size: 48,
            ..a.clone()
        };
        assert_ne!(a, b);
    }
}
