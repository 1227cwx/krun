use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HICON, IMAGE_ICON, LR_DEFAULTCOLOR, LR_SHARED, LoadImageW,
};

const ICON_RESOURCE_ID: usize = 1;

pub fn load_icon(size: i32) -> HICON {
    unsafe {
        LoadImageW(
            GetModuleHandleW(std::ptr::null()),
            ICON_RESOURCE_ID as *const u16,
            IMAGE_ICON,
            size,
            size,
            LR_DEFAULTCOLOR | LR_SHARED,
        ) as HICON
    }
}
