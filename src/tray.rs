use crate::win::wide;
use std::mem::{size_of, zeroed};
use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, MF_SEPARATOR, MF_STRING,
    SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP,
};

pub const TRAY_MESSAGE: u32 = WM_APP + 1;
pub const CMD_SHOW: u32 = 4101;
pub const CMD_OPEN_DIRECTORY: u32 = 4102;
pub const CMD_EXIT: u32 = 4103;
const TRAY_ID: u32 = 1;

pub struct TrayIcon {
    data: NOTIFYICONDATAW,
}

impl TrayIcon {
    pub fn add(hwnd: HWND, hotkey: &str) -> Result<Self, String> {
        let mut data: NOTIFYICONDATAW = unsafe { zeroed() };
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = TRAY_ID;
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.uCallbackMessage = TRAY_MESSAGE;
        data.hIcon = crate::brand::load_icon(20);
        set_tip(&mut data, hotkey);
        if unsafe { Shell_NotifyIconW(NIM_ADD, &data) } == 0 {
            return Err("无法创建通知区域图标".into());
        }
        Ok(Self { data })
    }

    pub fn update_tip(&mut self, hotkey: &str) {
        set_tip(&mut self.data, hotkey);
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &self.data) };
    }

    pub fn restore(&mut self, hotkey: &str) {
        set_tip(&mut self.data, hotkey);
        unsafe { Shell_NotifyIconW(NIM_ADD, &self.data) };
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unsafe { Shell_NotifyIconW(NIM_DELETE, &self.data) };
    }
}

pub fn show_menu(hwnd: HWND) -> u32 {
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return 0;
    }
    let show = wide("显示 / 隐藏");
    let directory = wide("打开配置目录");
    let exit = wide("退出");
    unsafe {
        AppendMenuW(menu, MF_STRING, CMD_SHOW as usize, show.as_ptr());
        AppendMenuW(
            menu,
            MF_STRING,
            CMD_OPEN_DIRECTORY as usize,
            directory.as_ptr(),
        );
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(menu, MF_STRING, CMD_EXIT as usize, exit.as_ptr());
        let mut point = POINT::default();
        GetCursorPos(&mut point);
        SetForegroundWindow(hwnd);
        let command = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            hwnd,
            std::ptr::null(),
        ) as u32;
        DestroyMenu(menu);
        command
    }
}

pub fn event_from_lparam(lparam: LPARAM) -> u32 {
    (lparam as usize & 0xffff) as u32
}

fn set_tip(data: &mut NOTIFYICONDATAW, hotkey: &str) {
    data.szTip.fill(0);
    let text = format!("KRun - {hotkey}");
    let value = wide(text);
    let length = value.len().min(data.szTip.len());
    data.szTip[..length].copy_from_slice(&value[..length]);
    data.szTip[data.szTip.len() - 1] = 0;
}
